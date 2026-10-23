<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { ContentTab } from '../content-tabs.svelte';
  import { addedLines, countsOf, parseDiff, relativeTo, type DiffRow } from '../diff';
  import { languageOf, readProjectFile } from '../files';
  import { fileWrites } from '../activity.svelte';
  import { inTauri } from '../history';
  import CodeView from './CodeView.svelte';
  import DevEmpty from './DevEmpty.svelte';
  import DiffView from './DiffView.svelte';

  let { file }: { file: Extract<ContentTab, { kind: 'file' }> } = $props();

  // Text is null until read: the area stays empty for that moment, with no spinner.
  let text = $state<string | null>(null);
  let error = $state('');
  // What changed in the file since the last commit, read from git; empty for a file git has no change for.
  // Raw: a diff can have thousands of rows, and none of them is edited in place.
  let rows = $state.raw<DiffRow[]>([]);
  let diffRead = $state(false);

  $effect(() => {
    const target = file;
    void fileWrites.tick;
    error = '';
    readProjectFile(target.root, target.path)
      .then((content) => {
        if (file === target) text = content;
      })
      .catch((failure: unknown) => {
        if (file === target) error = String(failure);
      });
  });

  $effect(() => {
    const target = file;
    void fileWrites.tick;
    if (!inTauri()) {
      diffRead = true;
      return;
    }
    void import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke<string>('change_diff', { root: target.root, path: relativeTo(target.root, target.path) }))
      .then((diff) => {
        if (file === target) rows = parseDiff(diff);
      })
      .catch(() => {
        if (file === target) rows = [];
      })
      .finally(() => {
        if (file === target) diffRead = true;
      });
  });

  const language = $derived(languageOf(file.name));
  const counts = $derived(countsOf(rows));
  const added = $derived(addedLines(rows));
  const changed = $derived(rows.length > 0);
  const view = $derived(file.view === 'changes' && (changed || !diffRead) ? 'changes' : 'file');
  const icon = (name: string) => name as ZIconName;
</script>

<div class="flex h-full min-h-0 flex-col bg-background" role="tabpanel" aria-label={file.name}>
  {#if changed}
    <!-- A file with changes can be read as it is, or as its change: removed lines red, added lines green. -->
    <div class="flex h-8 shrink-0 items-center gap-1 border-b border-border px-2 text-xs">
      <div class="flex items-center rounded-md border border-border p-px" role="radiogroup" aria-label="How to show the file">
        <button type="button" role="radio" aria-checked={view === 'file'} class="inline-flex h-5 cursor-pointer items-center gap-1 rounded px-2 outline-none {view === 'file' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" onclick={() => (file.view = 'file')}>
          <ZIcon zType={icon('lucideFileText')} zSize="12" /> File
        </button>
        <button type="button" role="radio" aria-checked={view === 'changes'} class="inline-flex h-5 cursor-pointer items-center gap-1 rounded px-2 outline-none {view === 'changes' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:bg-primary/10 hover:text-primary'}" onclick={() => (file.view = 'changes')}>
          <ZIcon zType={icon('lucideGitCompare')} zSize="12" /> Changes
        </button>
      </div>
      <span class="ml-1 font-mono text-emerald-500">+{counts.added}</span>
      <span class="font-mono text-red-500">−{counts.removed}</span>
    </div>
  {/if}
  {#if error}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType="lucideFile" class="size-8 opacity-60" />
      <p class="m-0 max-w-md break-all font-mono text-xs leading-relaxed">{file.path}</p>
      <p class="m-0 max-w-md text-xs leading-relaxed text-destructive">{error}</p>
    </div>
  {:else if view === 'changes'}
    {#if changed}
      <div class="min-h-0 flex-1"><DiffView {rows} lang={language} /></div>
    {:else if diffRead}
      <DevEmpty icon="lucideGitCompare" title="No changes" hint="This file is the same as in the last commit." />
    {/if}
  {:else if text !== null}
    <div class="min-h-0 flex-1"><CodeView text={text} lang={language} {added} /></div>
  {/if}
</div>
