<script lang="ts">
  import { ZButton, ZIcon, ZModal } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { contentTabs, saveTab, type ContentTab } from '../content-tabs.svelte';
  import { changeLines, countsOf, parseDiff, relativeTo, type DiffRow } from '../diff';
  import { languageLabel, languageOf, readProjectFile } from '../files';
  import { canFormat, prettifyStrict } from '../format';
  import { drafts, type Draft } from '../file-buffers';
  import { diffCache, fileKey, rememberText, textCache } from '../file-cache';
  import { MAX_EDIT_BYTES, splitFileText } from '../file-edit';
  import { toast } from '../toast';
  import { onDestroy, tick, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { fileWrites } from '../activity.svelte';
  import { inTauri } from '../history';
  import CodeEditor from './CodeEditor.svelte';
  import CodeView from './CodeView.svelte';
  import ChangeRuler from './ChangeRuler.svelte';
  import DevEmpty from './DevEmpty.svelte';
  import DiffView from './DiffView.svelte';
  import Markdown from './Markdown.svelte';

  let { file }: { file: Extract<ContentTab, { kind: 'file' }> } = $props();

  // The file as last read from or written to disk (raw: BOM and line ends included). Null until read: the area
  // stays empty for that moment, with no spinner.
  let disk = $state<string | null>(null);
  // What the editor starts with: the file, or the unsaved edits this tab kept while another tab was in front.
  let startDoc = $state('');
  let error = $state('');
  // What changed in the file since the last commit, read from git; empty for a file git has no change for.
  // Raw: a diff can have thousands of rows, and none of them is edited in place.
  // A viewer built again (the tab was left and came back) starts on what the last one knew, so it never opens empty.
  const seenDiff = untrack(() => diffCache.get(fileKey(file.root, file.path)));
  let rows = $state.raw<DiffRow[]>(seenDiff?.rows ?? []);
  let diffRead = $state(seenDiff !== undefined);
  let shownDiff = seenDiff?.diff;
  // Edited and not saved. Mirrored on the tab (its dot) and in `drafts` (so the edits outlive this viewer).
  let dirty = $state(false);
  // The file on disk is no longer what the edits started from: the raw new text, until Reload or Keep mine.
  let incoming = $state<string | null>(null);
  // The diff is being read again after a save: its added-line marks do not fit the file yet.
  let staleDiff = $state(false);
  let editor: CodeEditor | undefined = $state();
  let viewer: CodeView | undefined = $state();
  let lines = $state(0);

  const parsed = $derived(disk === null ? null : splitFileText(disk));
  // Too big (or not text) for the editor: shown in the read-only view.
  const readOnly = $derived(disk !== null && (disk.length > MAX_EDIT_BYTES || disk.includes('\0')));
  const readOnlyWhy = $derived(disk !== null && disk.includes('\0') ? 'Not plain text, so it cannot be edited' : 'Too large to edit');

  function syncDraft() {
    if (!dirty || disk === null) return void drafts.delete(file.id);
    const draft = drafts.get(file.id) ?? { disk, text: '' };
    draft.disk = disk;
    draft.read = () => editor?.getText() ?? draft.text;
    draft.saved = (raw) => {
      disk = raw;
      incoming = null;
      staleDiff = true;
      recheck();
    };
    drafts.set(file.id, draft);
  }
  function setDirty(value: boolean) {
    dirty = value;
    file.dirty = value;
    syncDraft();
  }
  /** After any change of the text or of the disk copy: is the editor still different from the file? */
  function recheck() {
    if (!editor || !parsed) return;
    const same = editor.equals(parsed.text);
    if (same !== !dirty) setDirty(!same);
    else if (dirty) syncDraft();
    lines = editor.lineCount();
  }

  const lineTotal = (text: string) => (text === '' ? 0 : text.split('\n').length);
  /** The file is on screen: its text, or the edits this tab kept (which then stand for the file). */
  function adopt(raw: string, kept?: Draft) {
    disk = raw;
    startDoc = kept ? kept.text : splitFileText(raw).text;
    lines = lineTotal(startDoc);
    if (kept) {
      dirty = true;
      file.dirty = true;
      syncDraft();
    }
  }
  // Built again after a visit to another tab: the kept edits, or the text read last time, are there at once. The
  // read below then tells whether the disk moved on meanwhile.
  untrack(() => {
    const kept = drafts.get(file.id);
    if (kept) return adopt(kept.disk, kept);
    const seen = textCache.get(fileKey(file.root, file.path));
    if (seen !== undefined) adopt(seen);
  });

  // An answer is matched to its file by the request, not by the tab object (which can be another proxy by the time
  // the answer comes), and one older than an answer already used is dropped. A viewer that is gone takes no answer.
  let readSeq = 0;
  let readApplied = 0;
  let destroyed = false;
  // Bumped by "Try again" to read the file once more.
  let attempt = $state(0);
  $effect(() => {
    const { id, root, path } = file;
    void fileWrites.tick;
    void attempt;
    const seq = ++readSeq;
    const key = fileKey(root, path);
    error = '';
    readProjectFile(root, path)
      .then((raw) => {
        if (seq < readApplied) return;
        readApplied = seq;
        rememberText(key, raw);
        if (destroyed) return;
        if (disk === null) {
          // First read: the edits this tab kept, if any, come back.
          const kept = drafts.get(id);
          adopt(kept?.disk ?? raw, kept);
          if (kept && raw !== kept.disk) incoming = raw;
          return;
        }
        if (raw === disk) return; // the app's own save, or nothing new: leave the cursor and scroll alone
        if (!dirty) {
          disk = raw;
          // The editor may not be built yet: it then starts on the new text.
          startDoc = splitFileText(raw).text;
          lines = lineTotal(startDoc);
          editor?.setText(startDoc);
        } else if ((editor?.getText() ?? drafts.get(id)?.text) === splitFileText(raw).text) {
          disk = raw;
          incoming = null;
          recheck();
        } else {
          incoming = raw; // never overwrite what the user typed
        }
      })
      .catch((failure: unknown) => {
        if (seq < readApplied || destroyed) return;
        readApplied = seq;
        textCache.delete(key);
        if (!dirty) error = String(failure);
      });
  });

  function reloadFromDisk() {
    if (incoming === null) return;
    disk = incoming;
    editor?.setText(splitFileText(incoming).text, true);
    incoming = null;
    recheck();
  }
  function keepMine() {
    if (incoming === null) return;
    disk = incoming;
    incoming = null;
    recheck();
  }

  // Leaving (another tab in front, or the tab closed): unsaved edits stay in `drafts` while the tab lives.
  onDestroy(() => {
    destroyed = true;
    const draft = drafts.get(file.id);
    if (!draft) return;
    if (!dirty || !contentTabs.tabs.some((tab) => tab.id === file.id)) return void drafts.delete(file.id);
    draft.text = editor?.getText() ?? draft.read?.() ?? draft.text;
    draft.read = undefined;
    draft.saved = undefined;
  });

  // Same rule as the read of the file: an answer older than one already applied is dropped, any other is used (so a
  // run of writes, each starting a new read, cannot keep every answer from ever showing). The answer also goes to
  // `diffCache`, even when this viewer is gone, for the next viewer of the file.
  let diffSeq = 0;
  let diffApplied = 0;
  $effect(() => {
    const { root, path } = file;
    void fileWrites.tick;
    if (!inTauri()) {
      diffRead = true;
      return;
    }
    const seq = ++diffSeq;
    const key = fileKey(root, path);
    const live = () => seq >= diffApplied && !destroyed;
    void import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<string>('change_diff', { root, path: relativeTo(root, path) }))
      .then((diff) => {
        if (seq < diffApplied) return;
        diffApplied = seq;
        // The same diff as on screen: keep the rows (and the scroll, the colours) as they are.
        if (diff === shownDiff) return;
        const next = parseDiff(diff);
        diffCache.set(key, { diff, rows: next });
        shownDiff = diff;
        if (!destroyed) rows = next;
      })
      .catch(() => {
        if (!live()) return;
        diffCache.delete(key);
        shownDiff = undefined;
        rows = [];
      })
      .finally(() => {
        if (live()) {
          diffRead = true;
          staleDiff = false;
        }
      });
  });

  const language = $derived(languageOf(file.name));
  const isMarkdown = $derived(language === 'markdown');
  const counts = $derived(countsOf(rows));
  const marks = $derived(changeLines(rows));
  const added = $derived(marks.added);
  const removed = $derived(marks.removed);
  const modified = $derived(marks.modified);
  const changed = $derived(rows.length > 0);
  const view = $derived(file.view === 'changes' && (changed || !diffRead) ? 'changes' : 'file');
  const icon = (name: string) => name as ZIconName;
  // The reason, without the "Error:" every Rust error string starts with, as a sentence.
  const reason = $derived(error.replace(/^(Error:\s*)+/i, '').replace(/^\w/, (c) => c.toUpperCase()));
  const folder = $derived(file.path.slice(0, Math.max(file.path.lastIndexOf('/'), file.path.lastIndexOf('\\')) + 1));
  let copied = $state(false);
  // ---------- toolbar: wrap, find, format, copy ----------
  let wrap = $state(true);
  let findOpen = $state(false);
  let query = $state('');
  let matchCase = $state(false);
  let wholeWord = $state(false);
  let useRegex = $state(false);
  let invalidQuery = $state(false);
  let previewOpen = $state(false);
  let previewText = $state('');
  let matchTotal = $state(0);
  let matchAt = $state(0);
  let findInput: HTMLInputElement | undefined = $state();
  let panel: HTMLDivElement | undefined = $state();
  let formatting = $state(false);
  let copiedAll = $state(false);

  $effect(() => {
    void query;
    void matchCase;
    void wholeWord;
    void useRegex;
    matchAt = 0;
  });
  $effect(() => {
    if (matchAt >= matchTotal) matchAt = 0;
  });

  const shownLines = $derived.by(() => {
    if (!readOnly && editor) return lines;
    const text = parsed?.text ?? '';
    return text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
  });
  const sizeLabel = $derived.by(() => {
    const bytes = new TextEncoder().encode(disk ?? '').length;
    return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  });
  // The path inside the project, with its middle cut when it is long.
  const shortPath = $derived.by(() => {
    const rel = relativeTo(file.root, file.path).replace(/\\/g, '/');
    return rel.length <= 48 ? rel : `${rel.slice(0, 18)}…${rel.slice(-27)}`;
  });
  const formattable = $derived(parsed !== null && !readOnly && canFormat(parsed.text, language));

  async function openFind() {
    findOpen = true;
    await tick();
    findInput?.focus();
    findInput?.select();
  }
  const step = (by: number) => {
    if (matchTotal > 0) matchAt = (matchAt + by + matchTotal) % matchTotal;
  };
  function save() {
    if (dirty) void saveTab(file.id);
  }
  function onFindKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      findOpen = false;
      editor?.focus();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
      event.preventDefault();
      save();
    }
  }
  // Ctrl+F while this viewer is on screen and focus is not in some other field.
  function onWindowKey(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'f' || view !== 'file' || disk === null || !panel || panel.offsetParent === null) return;
    const target = event.target as HTMLElement | null;
    if (target && /^(INPUT|TEXTAREA)$/.test(target.tagName) && !panel.contains(target)) return;
    event.preventDefault();
    void openFind();
  }

  // Format puts prettier's version into the editor as an ordinary edit: it is unsaved until Ctrl+S, Ctrl+Z reverts it.
  async function format() {
    const current = editor?.getText();
    if (current == null || !language) return;
    formatting = true;
    try {
      editor?.setText(await prettifyStrict(current, language), true);
    } catch (failure) {
      toast.error(`Could not format: ${String(failure).split('\n')[0].replace(/^(Error:\s*)+/i, '')}`);
    } finally {
      formatting = false;
    }
  }

  const currentText = () => (readOnly ? parsed?.text : editor?.getText()) ?? parsed?.text ?? '';
  function copyAll() {
    void navigator.clipboard.writeText(currentText()).then(() => {
      copiedAll = true;
      setTimeout(() => (copiedAll = false), 1500);
    });
  }

  let barWidth = $state(0);
  let compact = $state(false);
  // The find bar's buttons keep their own padding (`tool`); the toolbar's buttons turn icon-only when the labels do not fit.
  const toolBase = 'inline-flex h-7 cursor-pointer items-center gap-1.5 rounded text-xs outline-none disabled:opacity-40';
  const tool = $derived(`${toolBase} ${compact ? 'w-7 justify-center' : 'px-2'}`);
  const findTool = `${toolBase} px-2`;
  // ---------- toolbar width: labelled buttons while they fit, icons with a tooltip when they do not ----------
  const LABEL_WIDTH = { save: 78, unsaved: 70, wrap: 66, search: 76, preview: 80, format: 76, copy: 72 };
  const INFO_MIN = $derived(changed || !diffRead ? 230 : 24); // room kept beside the buttons before they lose their labels
  const labelledWidth = $derived(
    LABEL_WIDTH.wrap + LABEL_WIDTH.search + LABEL_WIDTH.copy + (dirty ? LABEL_WIDTH.save : 0) + (isMarkdown ? LABEL_WIDTH.preview : 0) + (formattable ? LABEL_WIDTH.format : 0) + 24,
  );
  $effect(() => {
    if (barWidth === 0) return;
    // A gap between the two limits, so a width on the edge does not flicker.
    if (!compact && barWidth < labelledWidth + INFO_MIN) compact = true;
    else if (compact && barWidth > labelledWidth + INFO_MIN + 24) compact = false;
  });
  /** The tooltip text: only while the buttons show no label. */
  const tip = (text: string) => (compact ? text : undefined);
  const toolOff = 'text-muted-foreground hover:bg-primary/10 hover:text-primary';
  const toolOn = 'bg-primary/15 text-primary';

  function copyPath() {
    void navigator.clipboard.writeText(file.path).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 1500);
    });
  }
</script>

{#snippet viewToggle()}
  <!-- A file with changes can be read as it is, or as its change: removed lines red, added lines green. -->
  <!-- Two equal halves with a pill that slides to the chosen one. -->
  <div class="relative grid h-6 shrink-0 grid-cols-2 rounded-md bg-muted p-0.5 text-xs" role="radiogroup" aria-label="How to show the file">
    <span class="pointer-events-none absolute inset-y-0.5 left-0.5 w-[calc(50%-2px)] rounded bg-background shadow-sm transition-transform duration-200 ease-out" style:transform={view === 'changes' ? 'translateX(100%)' : 'translateX(0)'} aria-hidden="true"></span>
    {#each [{ value: 'file', label: 'File', glyph: 'lucideFileText' }, { value: 'changes', label: 'Changes', glyph: 'lucideGitCompare' }] as item (item.value)}
      <button type="button" role="radio" aria-checked={view === item.value} class="relative z-10 inline-flex cursor-pointer items-center justify-center gap-1 rounded px-2.5 whitespace-nowrap outline-none transition-colors {view === item.value ? 'text-primary' : 'text-muted-foreground hover:text-foreground'}" onclick={() => (file.view = item.value as 'file' | 'changes')}>
        <ZIcon zType={icon(item.glyph)} zSize="12" /> {item.label}
      </button>
    {/each}
  </div>
  <span class="ml-1 font-mono text-emerald-500">+{counts.added}</span>
  <span class="font-mono text-red-500">−{counts.removed}</span>
{/snippet}

<svelte:window onkeydown={onWindowKey} />

<div bind:this={panel} class="relative flex h-full min-h-0 flex-col bg-background" role="tabpanel" aria-label={file.name}>
  {#if error}
    <div class="flex flex-1 items-center justify-center p-6">
      <div class="flex w-full max-w-md flex-col items-center gap-3 text-center">
        <span class="grid size-10 place-items-center rounded-full bg-destructive/10 text-destructive">
          <ZIcon zType={icon('lucideFileX')} class="size-5" />
        </span>
        <div class="flex flex-col gap-1">
          <p class="m-0 text-sm font-medium text-foreground">Can't open {file.name}</p>
          <p class="m-0 text-xs leading-relaxed text-muted-foreground">{reason}</p>
        </div>
        <p class="m-0 max-w-full truncate rounded-sm bg-muted px-2 py-1 font-mono text-[11px] text-muted-foreground" title={file.path}>{folder}<span class="text-foreground">{file.name}</span></p>
        <div class="flex gap-2">
          <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={() => attempt++}>
            <ZIcon zType={icon('lucideRotateCw')} class="size-3.5" /> Try again
          </ZButton>
          <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 gap-1.5 px-2.5 text-xs" onclick={copyPath}>
            <ZIcon zType={icon(copied ? 'lucideCheck' : 'lucideCopy')} class="size-3.5" /> {copied ? 'Copied' : 'Copy path'}
          </ZButton>
        </div>
      </div>
    </div>
  {:else if view === 'changes'}
    {#if changed || !diffRead}
      <!-- The bar is there from the first frame (its counts fade in once git has answered), so the area is never a blank band. -->
      <div class="flex h-9 shrink-0 items-center gap-1 border-b border-border px-3 text-xs"><div class="flex items-center gap-1 transition-opacity duration-200 {diffRead ? 'opacity-100' : 'opacity-0'}" aria-hidden={!diffRead}>{@render viewToggle()}</div></div>
      <div class="min-h-0 flex-1">
        {#if changed}<div class="h-full" in:fade={{ duration: 200 }}><DiffView {rows} lang={language} /></div>{/if}
      </div>
    {:else}
      <DevEmpty icon="lucideGitCompare" title="No changes" hint="This file is the same as in the last commit." />
    {/if}
  {:else if disk !== null}
    <div bind:clientWidth={barWidth} class="flex h-9 shrink-0 items-center gap-2 overflow-hidden border-b border-border px-3 text-xs">
      <div class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">{#if changed || !diffRead}<div class="flex items-center gap-1 transition-opacity duration-200 {diffRead ? 'opacity-100' : 'opacity-0'}" aria-hidden={!diffRead}>{@render viewToggle()}</div>{/if}</div>
      <div class="flex shrink-0 items-center gap-1">
        {#if dirty}
          <button type="button" class="{tool} {toolOn}" aria-label="Save" title={tip('Save (Ctrl+S)')} onclick={save}>
            <ZIcon zType={icon('lucideSave')} zSize="13" />{#if !compact} Save{/if}
          </button>
        {/if}
        <button type="button" class="{tool} {wrap ? toolOn : toolOff}" aria-pressed={wrap} aria-label="Wrap long lines" title={tip('Wrap long lines')} onclick={() => (wrap = !wrap)}>
          <ZIcon zType={icon('lucideType')} zSize="13" />{#if !compact} Wrap{/if}
        </button>
        <button type="button" class="{tool} {findOpen ? toolOn : toolOff}" aria-label="Find in file" title={tip('Find in file (Ctrl+F)')} onclick={() => (findOpen ? (findOpen = false) : void openFind())}>
          <ZIcon zType={icon('lucideSearch')} zSize="13" />{#if !compact} Search{/if}
        </button>
        {#if isMarkdown}
          <button type="button" class="{tool} {toolOff}" aria-label="Preview the rendered Markdown" title={tip('Preview the rendered Markdown')} onclick={() => ((previewText = currentText()), (previewOpen = true))}>
            <ZIcon zType={icon('lucideEye')} zSize="13" />{#if !compact} Preview{/if}
          </button>
        {/if}
        {#if formattable}
          <button type="button" class="{tool} {toolOff}" disabled={formatting} aria-label="Format the file" title={tip('Format the file (an edit: Ctrl+Z undoes it, Ctrl+S saves it)')} onclick={format}>
            <ZIcon zType={icon(formatting ? 'lucideLoaderCircle' : 'lucideBraces')} zSize="13" class={formatting ? 'animate-spin' : ''} />{#if !compact} Format{/if}
          </button>
        {/if}
        <button type="button" class="{tool} {toolOff}" aria-label="Copy the whole file" title={tip('Copy the whole file')} onclick={copyAll}>
          <ZIcon zType={icon(copiedAll ? 'lucideCheck' : 'lucideCopy')} zSize="13" />{#if !compact} {copiedAll ? 'Copied' : 'Copy'}{/if}
        </button>
      </div>
    </div>
    {#if incoming !== null}
      <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-muted px-3 text-xs" role="alert">
        <ZIcon zType={icon('lucideTriangleAlert')} zSize="13" class="shrink-0 text-amber-500" />
        <span class="min-w-0 truncate text-foreground">File changed on disk</span>
        <span class="flex-1"></span>
        <button type="button" class="{tool} {toolOff}" title="Replace your edits with the file on disk (Ctrl+Z brings them back)" onclick={reloadFromDisk}>Reload</button>
        <button type="button" class="{tool} {toolOff}" title="Keep your edits; saving overwrites the file on disk" onclick={keepMine}>Keep mine</button>
      </div>
    {/if}
    <div class="relative min-h-0 flex-1">
      {#if findOpen}
        <div class="absolute top-1.5 right-3 z-20 flex items-center gap-0.5 rounded-md border border-border bg-popover p-1 text-[13px] shadow-md" role="search">
          <input
            bind:this={findInput}
            bind:value={query}
            onkeydown={onFindKey}
            class="h-8 w-44 rounded-sm border bg-background px-2 text-[13px] outline-none focus:border-primary {invalidQuery ? 'border-destructive' : 'border-border'}"
            placeholder="Find"
            aria-label="Find in file"
            spellcheck="false"
          />
          <button type="button" class="{findTool} h-8 px-1.5 font-mono {matchCase ? toolOn : toolOff}" aria-pressed={matchCase} title="Match case" onclick={() => (matchCase = !matchCase)}>Aa</button>
          <button type="button" class="{findTool} h-8 px-1.5 font-mono {wholeWord ? toolOn : toolOff}" aria-pressed={wholeWord} title="Match whole word" onclick={() => (wholeWord = !wholeWord)}><span class="underline decoration-1 underline-offset-2">ab</span></button>
          <button type="button" class="{findTool} h-8 px-1.5 font-mono {useRegex ? toolOn : toolOff}" aria-pressed={useRegex} title="Use regular expression" onclick={() => (useRegex = !useRegex)}>.*</button>
          <span class="min-w-0 px-1.5 text-center text-xs whitespace-nowrap tabular-nums {invalidQuery ? 'text-destructive' : 'text-muted-foreground'}">{invalidQuery ? 'Invalid' : query ? (matchTotal ? `${matchAt + 1} of ${matchTotal}` : 'No results') : ''}</span>
          <button type="button" class="{findTool} h-8 px-1 {toolOff}" title="Previous match (Shift+Enter)" aria-label="Previous match" disabled={!matchTotal} onclick={() => step(-1)}><ZIcon zType={icon('lucideChevronUp')} zSize="14" /></button>
          <button type="button" class="{findTool} h-8 px-1 {toolOff}" title="Next match (Enter)" aria-label="Next match" disabled={!matchTotal} onclick={() => step(1)}><ZIcon zType={icon('lucideChevronDown')} zSize="14" /></button>
          <button type="button" class="{findTool} h-8 px-1 {toolOff}" title="Close (Esc)" aria-label="Close find" onclick={() => (findOpen = false)}><ZIcon zType={icon('lucideX')} zSize="14" /></button>
        </div>
      {/if}
      {#if !(dirty || staleDiff) && changed}
        <ChangeRuler total={shownLines} {added} {removed} {modified} ongo={(line) => (readOnly ? viewer?.goToLine(line) : editor?.goToLine(line))} />
      {/if}
      {#if readOnly}
        <CodeView bind:this={viewer} text={parsed?.text ?? ''} lang={language} {added} {removed} {modified} {wrap} query={findOpen ? query : ''} {matchCase} {wholeWord} regex={useRegex} bind:invalid={invalidQuery} current={matchAt} bind:total={matchTotal} />
      {:else}
        <CodeEditor bind:this={editor} doc={startDoc} lang={language} added={dirty || staleDiff ? undefined : added} removed={dirty || staleDiff ? undefined : removed} modified={dirty || staleDiff ? undefined : modified} {wrap} query={findOpen ? query : ''} {matchCase} {wholeWord} regex={useRegex} bind:invalid={invalidQuery} current={matchAt} bind:total={matchTotal} onchange={recheck} onsave={save} />
      {/if}
    </div>
    <!-- Where the file is and how big: path on the left; language, lines and size on the right. -->
    <div class="flex h-7 shrink-0 items-center gap-3 border-t border-border px-3 text-[11px] text-muted-foreground">
      <span class="min-w-0 flex-1 truncate font-mono" title={file.path}>{shortPath}</span>
      {#if readOnly}<span class="shrink-0 rounded-sm bg-muted px-1.5 py-px whitespace-nowrap">{readOnlyWhy}</span>{/if}
      <span class="shrink-0 rounded-sm bg-muted px-1.5 py-px">{languageLabel(language)}</span>
      <span class="shrink-0 whitespace-nowrap tabular-nums">{shownLines} lines · {sizeLabel}</span>
    </div>
  {/if}
</div>

{#if isMarkdown}
  <ZModal bind:open={previewOpen} zTitle={file.name} zWidth="860px" zBodyClass="max-h-[70vh] overflow-auto py-3" zOkText="Close" zCancelText={null} onOk={() => (previewOpen = false)}>
    <Markdown text={previewText} />
  </ZModal>
{/if}
