<script lang="ts">
  import PickerPopover, { type PickerItem } from '../ui/PickerPopover.svelte';
  import { ACTIVE_ON } from '../../app/ui/popover';
  import { createTextHistory } from '../../app/text/text-history';
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { ZIcon, ZPopover, ZSwitch, ZTabs, zTooltip } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem } from '@shival99/z-ui-svelte';
  import { OPENAI_SVG, OPENCODE_SVG } from '../../agents/brand-icons';
  import { activeSession, workspace } from '../../chat/sessions.svelte';
  import { keepPicture, pictureSrc, saveAttachment, type ImageSettings, type SuggestedKeyword } from '../../chat/agent';
  import { openImages } from '../../browser/preview.svelte';
  import { clearSuggestions, draft, gen, refreshHistory, runSuggest, startRun, stopRun } from '../../files/image-gen.svelte';
  import { findModel, freeFirst, modelSources, modelsReady, parseCustomModel, type ProviderId } from '../../agents/models.svelte';
  import { activeProviderKey, connectors, providerKeyEnv } from '../../agents/connectors.svelte';
  import { inTauri } from '../../app/platform/history';
  import { accountEpoch } from '../../chat/runs/account-epoch.svelte';
  import { toast } from '../../app/platform/toast';
  import SectionTitle from '../ui/SectionTitle.svelte';
  import TextEditContextMenu from '../ui/TextEditContextMenu.svelte';
  import AttachmentMenu from '../chat/AttachmentMenu.svelte';
  import { pictureMenuItems } from '../../chat/content/attachment-menu';
  import ImageHistoryDrawer from './ImageHistoryDrawer.svelte';
  import ImageResults from './ImageResults.svelte';
  import {
    EFFORT_LABEL,
    GEMINI_HOST,
    MAX_IMAGE_MODELS,
    MAX_REFERENCES,
    OPTION_ICON,
    PICTURE,
    SETTINGS,
    SETTING_CATEGORY,
    SETTING_ICON,
    fold,
    formatDuration,
    geminiModels,
    glide,
    openAiModels,
    readPicture,
    variantTabs,
    type ImageModel,
    type SettingKey,
  } from './image-generator-options';

  /** The enabled Gemini provider with a key (typed or in its environment variable), if the user set one up. */
  const geminiProvider = $derived(connectors.custom.find((item) => {
    const key = activeProviderKey(item);
    return item.enabled && item.baseUrl.includes(GEMINI_HOST) && (key?.apiKey || providerKeyEnv(item, key));
  }));
  const geminiKey = $derived(geminiProvider ? activeProviderKey(geminiProvider) : undefined);
  const geminiReady = $derived(!!geminiProvider);
  const gemini = $derived<ImageModel[]>(
    geminiProvider
      ? geminiModels.map((item) => ({
          ...item,
          backend: 'gemini' as const,
          custom: { kind: geminiProvider.kind, base_url: geminiProvider.baseUrl, api_key: geminiKey?.apiKey || null, key_env: providerKeyEnv(geminiProvider, geminiKey) || null },
        }))
      : [],
  );
  const models = $derived<ImageModel[]>([...openAiModels, ...gemini]);

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
  let promptArea = $state<HTMLTextAreaElement | undefined>();
  let textEditMenu = $state<{ openAt: (event: MouseEvent, textarea: HTMLTextAreaElement) => void } | undefined>();
  let attachmentMenu = $state<{ openAt: (event: MouseEvent, items: ZDropdownMenuItem[]) => void } | undefined>();
  // Ctrl+Z / Ctrl+Y also bring back a prompt that code changed (a suggestion, a keyword, a cleared field).
  const promptHistory = createTextHistory(draft.prompt);
  $effect(() => promptHistory.note(prompt));
  let selectedModels = $state<string[]>(draft.selectedModels ?? ['gpt-6-luna', 'gpt-image-2']);
  let variants = $state(draft.variants);
  let fast = $state(draft.fast);
  let effort = $state(draft.effort);
  // The run lives in the store, so it goes on while this panel is closed.
  const working = $derived(gen.working);
  const stopping = $derived(gen.stopping);
  // Time of the current or last run, shown beside the history button.
  const runMs = $derived(gen.runMs);

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

  const variantCount = $derived(Math.max(1, Math.min(4, Math.round(variants))));
  const candidateCount = $derived(selectedModels.length * variantCount);
  const settingsSummary = $derived(selectedModels.map((value) => models.find((item) => item.model === value)?.label ?? value).join(', ') || 'No model');
  // Models that can write the prompt suggestions: every source the chat's model picker lists (the agents and each enabled
  // provider, a new one too), so a provider or connector added in the settings shows here at once.
  const promptGroups = $derived(
    modelSources().map((source) => ({
      provider: source.key,
      agent: source.provider as ProviderId,
      label: source.label,
      icon: source.icon,
      logo: source.logo,
      items: source.models().map((item) => ({ value: item.value, label: item.label, hidden: item.hidden })),
    })),
  );
  const promptModels = $derived(promptGroups.flatMap((group) => group.items));
  // The effort levels follow the model that writes the suggestions, as they do in the chat picker.
  const promptSource = $derived(promptGroups.find((group) => group.items.some((item) => item.value === promptModel)));
  const promptProvider = $derived((promptSource?.agent ?? 'codex') as ProviderId);
  const effortTabs = $derived.by(() => {
    const levels: string[] = findModel(promptProvider, promptModel)?.efforts ?? ['low', 'medium', 'high', 'max'];
    return levels.map((value) => ({ value, label: EFFORT_LABEL[value] ?? value }));
  });
  $effect(() => {
    void accountEpoch[promptProvider];
    const levels = effortTabs.map((item) => item.value);
    if (!levels.includes(effort)) effort = findModel(promptProvider, promptModel)?.defaultEffort ?? levels[Math.min(1, levels.length - 1)];
  });

  const promptModelLabel = $derived(promptModels.find((item) => item.value === promptModel)?.label ?? promptModel);

  $effect(() => {
    void accountEpoch[promptProvider];
    if (Object.values(modelsReady).some((ready) => !ready)) return;
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
    // A custom provider's model is `<provider id>::<model>`: the call goes to its address with its own model name.
    const target = parseCustomModel(promptModel);
    const custom = target ? connectors.custom.find((item) => item.id === target.providerId) : undefined;
    void runSuggest({
      prompt: prompt.trim(),
      model: target?.model ?? promptModel,
      provider: custom ? 'custom' : promptProvider,
      custom: custom ? { kind: custom.kind, baseUrl: custom.baseUrl, apiKey: activeProviderKey(custom)?.apiKey || null, keyEnv: providerKeyEnv(custom) || null } : undefined,
      referencePaths,
      settings: settingsPayload(),
    });
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


  async function addPictures(files: File[]) {
    const pictures = files.filter((file) => PICTURE.test(file.type));
    if (!pictures.length) return;
    const room = MAX_REFERENCES - references.length;
    if (room <= 0) {
      toast.error(`Up to ${MAX_REFERENCES} reference images`);
      return;
    }
    for (const file of pictures.slice(0, room)) {
      const kept = await keepPicture({ id: crypto.randomUUID(), name: file.name || 'pasted image', mime: file.type, src: await readPicture(file) });
      references = [...references, kept];
    }
  }

  function onPaste(event: ClipboardEvent) {
    const files = [...(event.clipboardData?.files ?? [])];
    if (!files.some((file) => PICTURE.test(file.type))) return; // text pastes as usual
    event.preventDefault();
    void addPictures(files);
  }

  function onPromptKeydown(event: KeyboardEvent) {
    if (promptHistory.key(event, (text) => (prompt = text))) return;
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


  function timeLabel(millis: number) {
    return new Date(millis).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }


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

</script>

<!-- A ring that turns around a round button while its work runs, with a soft ripple; both stop for reduced motion. -->
{#snippet busyRing()}
  <span class="pointer-events-none absolute -inset-[2px] animate-spin rounded-full [background:conic-gradient(from_0deg,transparent_0_55%,var(--color-primary)_100%)] [mask:radial-gradient(farthest-side,transparent_calc(100%_-_2px),#000_calc(100%_-_2px))] motion-reduce:hidden" aria-hidden="true"></span>
  <span class="pointer-events-none absolute inset-0 animate-ping rounded-full bg-primary/25 [animation-duration:1.8s] motion-reduce:hidden" aria-hidden="true"></span>
{/snippet}

<div class="flex h-full min-h-0 flex-col" role="tabpanel" aria-label="Image generator">
  <header class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-1.5 text-[0.8125rem]">
    <span class="grid size-7 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType="lucideImage" class="size-4" /></span>
    <h2 class="m-0 min-w-0 flex-1 truncate text-sm font-semibold text-foreground">Image candidates</h2>
    {#if runMs > 0}
      <span class="inline-flex h-7 shrink-0 items-center rounded border border-border bg-background px-2.5 text-[0.6875rem] font-medium tabular-nums text-muted-foreground" title="Time of the last run" aria-live="off">{formatDuration(runMs)}</span>
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

  <ImageHistoryDrawer bind:open={historyOpen} />

  <div class="min-h-0 flex-1 overflow-hidden p-1.5">
    <div class="flex h-full min-h-0 flex-col gap-3">
      <div class="relative min-w-0 shrink-0 rounded-2xl border border-input bg-card p-2 shadow-lg transition-[border-color,box-shadow] focus-within:border-ring focus-within:ring-ring/20 focus-within:ring-2">
        {#if references.length > 0}
          <div class="mb-2 flex flex-wrap gap-2" aria-label="Reference images">
            {#each references as reference (reference.id)}
              <div
                class="group/ref relative"
                role="group"
                aria-label={reference.name}
                oncontextmenu={(event) =>
                  attachmentMenu?.openAt(
                    event,
                    pictureMenuItems(reference, {
                      preview: () => openImages(references, references.indexOf(reference)),
                      remove: () => (references = references.filter((item) => item !== reference)),
                    }),
                  )}
              >
                <button type="button" class="block size-12 cursor-zoom-in overflow-hidden rounded-lg border border-border outline-none transition hover:ring-2 hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring" aria-label="Preview {reference.name}" onclick={() => openImages(references, references.indexOf(reference))}>
                  <img src={pictureSrc(reference)} alt={reference.name} class="size-full object-cover" />
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
          bind:this={promptArea}
          bind:value={prompt}
          rows="2"
          aria-label="Image prompt"
          onkeydown={onPromptKeydown}
          onpaste={onPaste}
          oncontextmenu={(event) => promptArea && textEditMenu?.openAt(event, promptArea)}
          ondragover={(event) => event.preventDefault()}
          ondrop={onDrop}
          placeholder="Describe the image, style, composition and constraints…"
          class="max-h-45 min-h-6 w-[calc(100%+0.5rem)] resize-none overflow-x-hidden break-words border-0 bg-transparent p-0 pr-2 text-sm leading-normal outline-none placeholder:text-muted-foreground"
        ></textarea>
        <TextEditContextMenu bind:this={textEditMenu} />
        <AttachmentMenu bind:this={attachmentMenu} />

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
            class="flex h-8 min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-sm text-muted-foreground outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {settingsOpen ? 'bg-primary/10 text-primary' : ''}"
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
            <div class="flex w-full flex-col gap-4 overflow-x-hidden overflow-y-auto px-2.5 py-2.5 text-sm" role="menu" aria-label="Image settings" {@attach glide()}>
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
                    {@const activeProvider = promptSource?.provider === group.provider}
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
                      {:else if group.logo}
                        <img src={group.logo} alt="" class="size-3.5 shrink-0 object-contain" />
                      {:else}
                        <ZIcon zType={group.icon} class="size-3.5 shrink-0 text-muted-foreground" />
                      {/if}
                      <!-- As in the chat's model picker: the provider's name keeps its room, the model's gives way (truncated). -->
                      <span class="flex min-w-0 flex-1 items-baseline gap-2">
                        <span class="truncate {activeProvider ? 'max-w-[60%] shrink-0' : 'min-w-0 flex-1'}">{group.label}</span>
                        {#if activeProvider}<span class="min-w-0 flex-1 truncate text-right text-[0.6875rem] text-muted-foreground">{promptModelLabel}</span>{/if}
                      </span>
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
                  <div class="flex items-center gap-1.5 text-sm font-medium">
                    <ZIcon zType="lucideZap" class="size-3.5 text-warning" />
                    Fast mode
                  </div>
                  <div class="mt-1 text-[0.6875rem] leading-snug text-muted-foreground">Faster output, higher usage</div>
                </div>
                <ZSwitch zVariant="dot" bind:checked={fast} zSize="sm" />
              </section>
            </div>
          {/snippet}
        </ZPopover>

        <ZPopover anchor={modelsRow} bind:open={modelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={203}>
          {#snippet children()}
            <div class="flex w-full flex-col gap-0.5 p-1.5 text-sm" role="menu" aria-label="Image models" tabindex="-1" onpointerenter={keepNestedMenusOpen} onpointerleave={closeNestedMenusSoon} {@attach glide()}>
              <div class="mr-1 flex shrink-0 items-center gap-2 px-1.5 py-1 text-[10px] font-medium tracking-wide text-primary uppercase" role="separator"><span>Codex</span><span class="h-px flex-1 bg-primary"></span></div>
              {#each openAiModels as item (item.model)}
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
              <div class="mr-1 mt-1 flex shrink-0 items-center gap-2 px-1.5 py-1 text-[10px] font-medium tracking-wide text-primary uppercase" role="separator"><span>Google Gemini</span><span class="h-px flex-1 bg-primary"></span></div>
              {#if geminiReady}
              {#each gemini as item (item.model)}
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
                  <ZIcon zType="lucideSparkles" class="size-3.5 shrink-0 text-foreground" />
                  <span class="min-w-0 flex-1 truncate">{item.label}</span>
                  {#if active}<ZIcon zType="lucideCheck" class="size-3.5 shrink-0" />{/if}
                </button>
              {/each}
              {:else}
                <p class="m-0 px-1.5 py-1 text-[0.6875rem] leading-snug text-muted-foreground">Add a Gemini API key in Settings, Cloud providers, to use Nano Banana.</p>
              {/if}
            </div>
          {/snippet}
        </ZPopover>

        <ZPopover anchor={promptModelsAnchor} bind:open={promptModelsOpen} zTrigger="manual" zPosition="right-top" zOffset={6} zWidth={203}>
          {#snippet children()}
            {@const selectedGroup = promptGroups.find((group) => group.provider === promptSubProvider)}
            {@const sorted = freeFirst(selectedGroup?.items ?? [])}
            <div class="flex w-full flex-col gap-0.5 overflow-x-hidden overflow-y-auto py-1 pl-1 text-sm" style:max-height="320px" role="radiogroup" aria-label="Prompt models" tabindex="-1" onpointerenter={keepNestedMenusOpen} onpointerleave={closeNestedMenusSoon} {@attach glide()}>
              {#if selectedGroup && !modelsReady[selectedGroup.agent]}
                <!-- The CLI's own list is still being read: rows that stand where its models will, instead of a list that would be replaced. -->
                <div class="flex w-full flex-col gap-1 px-2 py-2" role="status" aria-label="Loading models">
                  {#each [70, 55, 82, 48] as width (width)}<span class="h-4 animate-pulse rounded bg-muted" style:width="{width}%"></span>{/each}
                </div>
              {:else if !selectedGroup?.items.length}
                <!-- The same empty state as the chat's model picker: the provider listed no models. -->
                <div class="flex w-full flex-col items-center gap-1.5 px-2 py-3 text-center">
                  <ZIcon zType="lucideShieldAlert" class="size-5 text-muted-foreground" />
                  <div class="text-sm font-medium">No AI models</div>
                  <p class="m-0 text-[0.6875rem] leading-snug text-muted-foreground">{selectedGroup?.label ?? 'This provider'} returned no models, or its provider has an error. Check it in Settings.</p>
                </div>
              {/if}
              {#each selectedGroup && modelsReady[selectedGroup.agent] ? sorted.items : [] as item, index (item.value)}
                {@const active = item.value === promptModel}
                {#each sorted.dividers.filter((divider) => divider.at === index) as divider (divider.label)}<div class="mr-1 mt-1 flex shrink-0 items-center gap-2 px-2 text-[10px] font-medium tracking-wide text-primary uppercase" class:text-green-500={divider.label === 'Free'} class:text-red-500={divider.label === 'Paid'} class:text-muted-foreground={divider.label === 'Hide'} role="separator"><span>{divider.label}</span><span class="h-px flex-1 bg-primary" class:bg-green-500={divider.label === 'Free'} class:bg-red-500={divider.label === 'Paid'} class:bg-muted-foreground={divider.label === 'Hide'}></span></div>{/each}
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
          <div transition:fold class="-mx-1.5 -mb-2 mt-2 flex flex-col gap-1 overflow-hidden border-t border-border px-1.5 pt-2 pb-2" aria-label="Prompt suggestions">
            <span class="px-1 text-[0.6875rem] text-muted-foreground">Suggested prompts · click to use</span>
            {#if suggesting}
              <div class="flex flex-col gap-2 px-1 py-1" role="status" aria-label="Writing prompt suggestions">
                {#each [0, 1, 2] as line (line)}
                  <div class="h-3 animate-pulse rounded bg-muted {line === 2 ? 'w-3/5' : 'w-full'}"></div>
                {/each}
              </div>
            {:else}
              {#each promptSuggestions as suggestion (suggestion)}
                <button in:fade={{ duration: 220 }} type="button" class="rounded-md bg-muted/50 px-2 py-1.5 text-left text-sm leading-4 text-foreground transition-colors hover:bg-primary/10 hover:text-primary focus-visible:bg-primary/10 focus-visible:text-primary outline-none" onclick={() => useSuggestion(suggestion)}>
                  {suggestion}
                </button>
              {/each}
              {#if suggestedKeywords.length > 0}
                <div class="flex flex-wrap gap-1 px-1 pt-1" aria-label="Suggested keywords">
                  {#each suggestedKeywords as keyword (keyword.category + keyword.value)}
                    <button type="button" class="rounded-full border border-primary/30 bg-primary/5 px-2 py-0.5 text-[0.6875rem] text-primary outline-none transition-colors hover:bg-primary/15 focus-visible:ring-2 focus-visible:ring-ring" title={SETTINGS.find((item) => item.key === SETTING_CATEGORY[keyword.category])?.label ?? 'Prompt keyword'} onclick={() => useKeyword(keyword)}>
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
      <div class="flex shrink-0 flex-wrap items-center gap-1.5" role="group" aria-label="Image settings">
        {#each SETTINGS as setting (setting.key)}
          {@const chosen = settings[setting.key]}
          <button
            type="button"
            class="flex h-8 min-w-0 cursor-pointer items-center gap-1.5 rounded-md border border-border bg-background px-3 text-sm outline-none transition-colors hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring {settingOpen && settingKey === setting.key ? ACTIVE_ON : ''} {chosen ? 'text-foreground' : 'text-muted-foreground'}"
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

      <div class="-mx-1.5 flex min-h-0 flex-1 flex-col border-t border-border pt-3">
        <div class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-1.5" role="region" aria-label="Image candidate results">
          <ImageResults />
        </div>
      </div>
    </div>
  </div>
</div>
