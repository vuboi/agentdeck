<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { ZDrawer, ZIcon, ZPopover, ZSwitch, ZTabs, createVirtualizer, zHoverGlide, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { OPENAI_SVG, OPENCODE_SVG } from '../brand-icons';
  import { activeSession, draftImages, workspace, type Attachment } from '../sessions.svelte';
  import {
    generateImages,
    listImageHistory,
    readImageDataUrl,
    revealFile,
    saveAttachment,
    stopImageGeneration,
    suggestImagePrompts,
    type GeneratedImage,
    type ImageHistoryRun,
    type ImageModelChoice,
    type ImageSettings,
    type SuggestedKeyword,
  } from '../agent';
  import { openImages } from '../preview.svelte';
  import { findModel, modelCatalog, modelLabel, type ProviderId } from '../models.svelte';
  import { inTauri } from '../history';
  import { toast } from '../toast';
  import DevEmpty from './DevEmpty.svelte';
  import SectionTitle from './SectionTitle.svelte';

  type ImageModel = ImageModelChoice & { label: string };
  type Preview = GeneratedImage & { src: string };

  const models: ImageModel[] = [
    { model: 'gpt-6-luna', backend: 'responses', label: 'GPT 6 Luna' },
    { model: 'gpt-image-2', backend: 'images', label: 'GPT Image 2' },
  ];
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
  let references = $state<Reference[]>([]);
  let settings = $state<Record<SettingKey, string>>({ style: '', domain: '', audience: '', ratio: '', resolution: '' });
  let settingKey = $state<SettingKey | null>(null);
  let settingAnchor = $state<HTMLElement | null>(null);
  let settingOpen = $state(false);
  let suggestedKeywords = $state<SuggestedKeyword[]>([]);
  const currentOptions = $derived(SETTINGS.find((item) => item.key === settingKey)?.options ?? []);
  let settingQuery = $state('');
  // The option the arrow keys are on: moved by the arrows and by a new search, picked with Enter.
  let cursor = $state(0);
  // The options whose name holds the search text; Auto is listed only while nothing is searched.
  const shownOptions = $derived.by(() => {
    const query = settingQuery.trim().toLowerCase();
    const matches = currentOptions.filter((option) => option.toLowerCase().includes(query));
    return query ? matches : ['', ...matches];
  });

  let prompt = $state('');
  let selectedModels = $state<string[]>(models.map((item) => item.model));
  let variants = $state(2);
  let fast = $state(true);
  let effort = $state('medium');
  let working = $state(false);
  let stopping = $state(false);
  let previews = $state<Preview[]>([]);
  let failureLines = $state<string[]>([]);
  let failuresOpen = $state(false);
  // Time of the current or last run, shown beside the history button.
  let runMs = $state(0);
  let loadingRun = $state(0);
  let runStart = 0;
  let runTimer: ReturnType<typeof setInterval> | undefined;

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
  let history = $state<ImageHistoryRun[]>([]);
  let activeRunId = $state<string | null>(null);
  let promptSuggestions = $state<string[]>([]);
  let promptModel = $state('gpt-6-luna');
  let suggesting = $state(false);
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
  // The skeleton of a run is fixed when it starts: changing the settings only shapes the next run.
  let runSkeleton = $state(2);
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
    if (inTauri()) void refreshHistory();
  });

  async function refreshHistory() {
    try {
      history = await listImageHistory();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  function selected() {
    return models.filter((item) => selectedModels.includes(item.model));
  }

  function toggleModel(model: string) {
    selectedModels = selectedModels.includes(model) ? selectedModels.filter((value) => value !== model) : [...selectedModels, model];
  }

  async function withPreviews(images: GeneratedImage[]): Promise<Preview[]> {
    const loaded = await Promise.all(
      images.map(async (image) => {
        try {
          return { ...image, src: await readImageDataUrl(image.path) };
        } catch {
          return null;
        }
      }),
    );
    return loaded.filter((image): image is Preview => image !== null);
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
    suggesting = true;
    promptSuggestions = [];
    suggestedKeywords = [];
    try {
      const result = await suggestImagePrompts({ prompt: prompt.trim(), model: promptModel, referencePaths: await referencePaths(), settings: settingsPayload() });
      promptSuggestions = result.suggestions;
      suggestedKeywords = result.keywords;
      if (!promptSuggestions.length) toast.error('The prompt model returned no suggestions');
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      suggesting = false;
    }
  }

  function useSuggestion(value: string) {
    prompt = value;
    promptSuggestions = [];
    suggestedKeywords = [];
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
    cursor = Math.max(0, shownOptions.indexOf(settings[key]));
    settingOpen = true;
  }

  // Arrow keys move through the options, Enter picks the one on, Escape closes the list.
  function onSettingKey(event: KeyboardEvent) {
    const count = shownOptions.length;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!count) return;
      cursor = (cursor + (event.key === 'ArrowDown' ? 1 : count - 1)) % count;
      const list = (event.currentTarget as HTMLElement).closest('[role="menu"]');
      void tick().then(() => list?.querySelector(`[data-option-index="${cursor}"]`)?.scrollIntoView({ block: 'nearest' }));
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (count) pickSetting(shownOptions[cursor]);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      settingOpen = false;
    }
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

  let fileInput = $state<HTMLInputElement | undefined>();

  function onDrop(event: DragEvent) {
    const files = [...(event.dataTransfer?.files ?? [])];
    if (!files.some((file) => PICTURE.test(file.type))) return;
    event.preventDefault();
    void addPictures(files);
  }

  async function generate() {
    if (!inTauri()) {
      toast.error('Image generation needs the desktop app');
      return;
    }
    const choices = selected();
    if (!prompt.trim()) {
      toast.error('Write an image prompt first');
      return;
    }
    if (!choices.length) {
      toast.error('Choose at least one image model');
      return;
    }
    runSkeleton = Math.max(2, Math.min(6, choices.length * variantCount));
    working = true;
    runStart = performance.now();
    runMs = 0;
    clearInterval(runTimer);
    runTimer = setInterval(() => (runMs = performance.now() - runStart), 100);
    stopping = false;
    previews = [];
    failureLines = [];
    activeRunId = null;
    try {
      const result = await generateImages({ prompt, models: choices, variants: variantCount, cwd: projectPath, referencePaths: await referencePaths(), settings: settingsPayload() });
      activeRunId = result.run_id;
      failureLines = result.failures.map((failure) => `${failure.model} #${failure.variant}: ${failure.error}`);
      previews = await withPreviews(result.images);
      if (!previews.length && failureLines.length) toast.error('No image was generated. Check the details below.');
      void refreshHistory();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      clearInterval(runTimer);
      runMs = performance.now() - runStart;
      working = false;
      stopping = false;
    }
  }

  async function stop() {
    stopping = true;
    try {
      await stopImageGeneration();
    } catch (error) {
      stopping = false;
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  async function openRun(run: ImageHistoryRun) {
    activeRunId = run.id;
    failureLines = run.failures;
    previews = [];
    // Skeletons until the saved files are read back, so the click answers at once.
    loadingRun = run.images.length;
    try {
      previews = await withPreviews(
        run.images.map((image) => ({ model: image.model, backend: image.backend, path: image.path, variant: image.variant })),
      );
    } finally {
      loadingRun = 0;
    }
  }

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

<div class="flex h-full min-h-0 flex-col" role="tabpanel" aria-label="Image generator">
  <header class="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-[13px]">
    <span class="grid size-7 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType="lucideImage" class="size-4" /></span>
    <h2 class="m-0 min-w-0 flex-1 truncate text-xs font-semibold text-foreground">Image candidates</h2>
    {#if runMs > 0}
      <span class="shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground" title="Time of the last run" aria-live="off">{formatDuration(runMs)}</span>
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
              class="grid size-7 cursor-pointer place-items-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
              aria-label="AI suggestions"
              title="AI suggestions"
              disabled={(!prompt.trim() && references.length === 0) || !promptModel}
              aria-busy={suggesting}
              onclick={() => void suggestPrompts()}
            >
              {#if suggesting}
                <ZIcon zType="lucideLoaderCircle" class="size-3.5 animate-spin" />
              {:else}
                <ZIcon zType="lucideWandSparkles" class="size-3.5" />
              {/if}
            </button>

            {#if working}
              <button
                type="button"
                class="grid size-7 cursor-pointer place-items-center rounded-full bg-primary text-primary-foreground transition-opacity outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
                aria-label="Stop generating"
                title="Stop"
                disabled={stopping}
                onclick={() => void stop()}
              >
                <svg class="size-2.5" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="2.5" width="11" height="11" rx="2.5" fill="currentColor" /></svg>
              </button>
            {:else}
              <button
                type="button"
                class="grid size-7 cursor-pointer place-items-center rounded-full bg-primary text-primary-foreground transition-opacity outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
                aria-label="Generate images"
                title="Generate {candidateCount} image candidates"
                disabled={!prompt.trim() || selectedModels.length === 0}
                onclick={() => void generate()}
              >
                <ZIcon zType="lucideArrowUp" class="size-3.5" />
              </button>
            {/if}
          </div>
        </div>

        <ZPopover anchor={settingsTrigger} bind:open={settingsOpen} zTrigger="manual" zPosition="top-left" zOffset={8} zWidth={300}>
          {#snippet children()}
            <div class="flex w-full flex-col gap-4 overflow-y-auto px-2.5 py-2.5 text-xs" role="menu" aria-label="Image settings" {@attach glide()}>
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
                      class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {activeProvider ? 'bg-primary/10' : ''}"
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

        <ZPopover anchor={modelsRow} bind:open={modelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={148}>
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

        <ZPopover anchor={promptModelsAnchor} bind:open={promptModelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={168}>
          {#snippet children()}
            {@const selectedGroup = promptGroups.find((group) => group.provider === promptSubProvider)}
            <div class="flex w-full flex-col gap-0.5 overflow-y-auto py-1 pl-1 text-xs" style:max-height="320px" role="radiogroup" aria-label="Prompt models" tabindex="-1" onpointerenter={keepNestedMenusOpen} onpointerleave={closeNestedMenusSoon} {@attach glide()}>
              {#if !selectedGroup?.items.length}
                <!-- The same empty state as the chat's model picker: the provider listed no models. -->
                <div class="flex w-full flex-col items-center gap-1.5 px-3 py-4 text-center">
                  <ZIcon zType="lucideShieldAlert" class="size-5 text-muted-foreground" />
                  <div class="text-xs font-medium">No AI models</div>
                  <p class="m-0 text-[11px] leading-snug text-muted-foreground">{selectedGroup?.label ?? 'This provider'} returned no models, or its provider has an error. Check it in Settings.</p>
                </div>
              {/if}
              {#each selectedGroup?.items ?? [] as item (item.value)}
                {@const active = item.value === promptModel}
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
          <div class="-mx-3 -mb-2 mt-2 flex flex-col gap-1 border-t border-border px-3 pt-2 pb-2" aria-label="Prompt suggestions">
            <span class="px-1 text-[11px] text-muted-foreground">Suggested prompts · click to use</span>
            {#if suggesting}
              <div class="flex flex-col gap-2 px-1 py-1" role="status" aria-label="Writing prompt suggestions">
                {#each [0, 1, 2] as line (line)}
                  <div class="h-3 animate-pulse rounded bg-muted {line === 2 ? 'w-3/5' : 'w-full'}"></div>
                {/each}
              </div>
            {:else}
              {#each promptSuggestions as suggestion (suggestion)}
                <button type="button" class="rounded-md bg-muted/50 px-2 py-1.5 text-left text-xs leading-4 text-foreground transition-colors hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10 focus-visible:text-primary outline-none" onclick={() => useSuggestion(suggestion)}>
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
            class="flex h-7 min-w-0 cursor-pointer items-center gap-1 rounded border border-border bg-background px-2.5 text-[11px] outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {settingOpen && settingKey === setting.key ? 'bg-primary/10 text-primary' : ''} {chosen ? 'text-foreground' : 'text-muted-foreground'}"
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

      <ZPopover anchor={settingAnchor} bind:open={settingOpen} zTrigger="manual" zPosition="bottom-left" zOffset={6} zWidth={200}>
        {#snippet children()}
          <div class="flex max-h-72 w-full flex-col text-xs" role="menu" aria-label="Setting options">
            <label class="flex shrink-0 items-center gap-2 border-b border-border px-2.5 py-2 text-muted-foreground focus-within:text-foreground">
              <ZIcon zType="lucideSearch" class="size-3.5 shrink-0" />
              <!-- svelte-ignore a11y_autofocus -->
              <input bind:value={settingQuery} oninput={() => (cursor = 0)} onkeydown={onSettingKey} autofocus type="text" placeholder="Search..." aria-label="Search options" class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />
            </label>
            <div class="flex min-h-0 flex-col gap-0.5 overflow-y-auto p-1.5">
            {#each shownOptions as option, index (option)}
              {@const active = (settingKey ? settings[settingKey] : '') === option}
              <button
                type="button"
                role="menuitemradio"
                aria-checked={active}
                data-option-index={index}
                class="flex w-full cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-left outline-none hover:bg-primary/10 focus-visible:bg-primary/10 {active ? 'bg-primary/10 text-primary' : index === cursor ? 'bg-primary/10' : ''}"
                onclick={() => pickSetting(option)}
              >
                {#if OPTION_ICON[option]}<ZIcon zType={OPTION_ICON[option]} class="size-3.5 shrink-0 text-muted-foreground" />{/if}
                <span class="min-w-0 flex-1 truncate">{option || 'Auto'}</span>
                {#if active}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0" />{/if}
              </button>
            {/each}
            </div>
          </div>
        {/snippet}
      </ZPopover>

      <div class="-mx-3 flex min-h-0 flex-1 flex-col border-t border-border pt-3">
        <div class="flex min-h-0 flex-1 flex-col px-3">
          {#if working || loadingRun}
            <div class="grid grid-cols-2 gap-2" role="status" aria-label="Generating image candidates">
              {#each Array.from({ length: working ? runSkeleton : loadingRun }) as _, index (index)}
                <div class="overflow-hidden rounded-lg border border-border bg-background shadow-sm">
                  <div class="relative aspect-square overflow-hidden bg-muted/60">
                    <div class="absolute inset-0 animate-pulse bg-gradient-to-br from-muted via-primary/10 to-muted"></div>
                  </div>
                  <div class="flex items-center gap-2 px-2 py-2">
                    <div class="h-2.5 w-2/5 animate-pulse rounded-full bg-muted"></div>
                  </div>
                </div>
              {/each}
            </div>
          {:else if previews.length}
            <div class="grid grid-cols-2 gap-2" aria-label="Generated image candidates">
              {#each previews as image, index (image.path)}
                <article class="group overflow-hidden rounded-lg border border-border bg-background">
                  <button type="button" class="block w-full cursor-zoom-in outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {image.model} #{image.variant}" onclick={() => openPreview(index)}>
                    <img src={image.src} alt="Generated candidate" class="aspect-square w-full object-cover" />
                  </button>
                  <div class="flex items-center gap-1 px-1.5 py-1.5">
                    <span class="min-w-0 flex-1 truncate text-[10px] text-muted-foreground" title={image.path}>{image.model} · #{image.variant}</span>
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
