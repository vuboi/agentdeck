<script lang="ts">
  import { tick } from 'svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { officeFormatOf, readProjectFileBytes } from '../../files/files';
  import { displayPath } from '../../files/paths';
  import { fileWrites } from '../../chat/runs/activity.svelte';
  import {
    parseOfficeWorkbook,
    renderOfficeDocument,
    type OfficeWorkbookPreview,
  } from '../../files/office-preview';
  import type { ContentTab } from '../../app/ui/content-tabs.svelte';
  import { revealTab } from '../../app/ui/tab-strip';
  import TabListButton from '../ui/TabListButton.svelte';
  import type { PickerItem } from '../ui/PickerPopover.svelte';

  let { file }: { file: Extract<ContentTab, { kind: 'file' }> } = $props();
  let scroll: HTMLDivElement | undefined = $state();
  let sheetStrip: HTMLDivElement | undefined = $state();
  let sheetStripWidth = $state(0);
  let sheetsOverflow = $state(false);
  let sheetScrolled = $state(false);
  let documentHost: HTMLDivElement | undefined = $state();
  let workbook = $state.raw<OfficeWorkbookPreview | null>(null);
  let activeSheet = $state('');
  let loading = $state(true);
  let error = $state('');
  let attempt = $state(0);

  const format = $derived(officeFormatOf(file.name));
  const shortPath = $derived(displayPath(file.root, file.path).replace(/\\/g, '/'));
  const label = $derived(
    format === 'docx'
      ? 'Word document'
      : format === 'xlsx'
        ? 'Excel workbook'
        : 'PowerPoint presentation',
  );
  const icon = (name: string) => name as ZIconName;
  const selectedSheet = $derived(
    workbook?.sheets.find((sheet) => sheet.name === activeSheet) ??
      workbook?.sheets[0],
  );
  const sheetItems = $derived<PickerItem<string>[]>(
    (workbook?.sheets ?? []).map((sheet) => ({
      id: sheet.name,
      label: sheet.name,
      selected: activeSheet === sheet.name,
      data: sheet.name,
    })),
  );

  function measureSheetStrip() {
    if (!sheetStrip) return;
    sheetsOverflow = sheetStrip.scrollWidth > sheetStrip.clientWidth + 1;
    sheetScrolled = sheetStrip.scrollLeft > 1;
  }

  function scrollSheetStrip(event: WheelEvent) {
    if (!sheetStrip || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    event.preventDefault();
    sheetStrip.scrollLeft += event.deltaY;
  }

  function chooseSheet(name: string) {
    activeSheet = name;
    void tick().then(() => {
      revealTab(sheetStrip, name);
      measureSheetStrip();
    });
  }

  $effect(() => {
    const strip = sheetStrip;
    void sheetStripWidth;
    void workbook?.sheets.length;
    if (!strip) return;
    const observer = new ResizeObserver(measureSheetStrip);
    observer.observe(strip);
    for (const tab of strip.children) observer.observe(tab);
    void tick().then(measureSheetStrip);
    return () => observer.disconnect();
  });

  $effect(() => {
    const { root, path, name } = file;
    const currentFormat = officeFormatOf(name);
    const target = documentHost;
    const scrollElement = scroll;
    void fileWrites.tick;
    void attempt;
    if (!currentFormat || !target || !scrollElement) return;

    let current = true;
    let dispose: (() => void) | undefined;
    loading = true;
    error = '';
    workbook = null;
    activeSheet = '';

    void readProjectFileBytes(root, path)
      .then(async (bytes) => {
        if (!current) return;
        if (currentFormat === 'xlsx') {
          const parsed = parseOfficeWorkbook(bytes);
          workbook = parsed;
          activeSheet = parsed.sheets[0]?.name ?? '';
          target.replaceChildren();
          return;
        }
        const cleanup = await renderOfficeDocument(
          currentFormat,
          bytes,
          target,
          scrollElement,
        );
        if (current) dispose = cleanup;
        else cleanup();
      })
      .catch((failure: unknown) => {
        if (current) error = String(failure).replace(/^(Error:\s*)+/i, '');
      })
      .finally(() => {
        if (current) loading = false;
      });

    return () => {
      current = false;
      dispose?.();
    };
  });

  function columnName(index: number): string {
    let value = index + 1;
    let name = '';
    while (value > 0) {
      value--;
      name = String.fromCharCode(65 + (value % 26)) + name;
      value = Math.floor(value / 26);
    }
    return name;
  }

  function handleWheel(event: WheelEvent) {
    if (!event.shiftKey || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    if (!scroll) return;
    scroll.scrollLeft += event.deltaY;
    event.preventDefault();
  }
</script>

<div
  class="flex h-full min-h-0 flex-col {format === 'xlsx' ? 'bg-white text-zinc-950 dark:bg-black dark:text-white' : 'bg-background'}"
  aria-label="{label} preview"
>
  <div
    class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3"
  >
    <span
      class="grid size-6 shrink-0 place-items-center rounded-md {format ===
      'docx'
        ? 'bg-blue-500/10 text-blue-600'
        : format === 'xlsx'
          ? 'bg-emerald-500/10 text-emerald-600'
          : 'bg-orange-500/10 text-orange-600'}"
    >
      <ZIcon
        zType={icon(
          format === 'xlsx'
            ? 'lucideTable2'
            : 'lucideFileText',
        )}
        zSize="14"
      />
    </span>
    <span
      class="min-w-0 flex-1 truncate text-xs font-medium text-foreground"
      title={file.name}>{file.name}</span
    >
    <span
      class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground"
      >{format}</span
    >
    {#if error}
      <button
        type="button"
        class="inline-flex h-7 items-center gap-1 rounded px-2 text-xs text-muted-foreground hover:bg-primary/10 hover:text-primary"
        onclick={() => attempt++}
      >
        <ZIcon zType={icon('lucideRotateCw')} zSize="12" /> Try again
      </button>
    {/if}
  </div>

  {#if format === 'xlsx' && workbook?.sheets.length}
    <div class="flex h-9 shrink-0 items-center gap-1 border-b border-border px-1.5">
      <div class="relative flex min-w-0 flex-1 items-center">
        <div
          bind:this={sheetStrip}
          bind:clientWidth={sheetStripWidth}
          class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden"
          role="tablist"
          aria-label="Workbook sheets"
          onscroll={measureSheetStrip}
          onwheel={scrollSheetStrip}
        >
          {#each workbook.sheets as sheet (sheet.name)}
            <button
              type="button"
              data-tab-id={sheet.name}
              role="tab"
              aria-selected={activeSheet === sheet.name}
              class="max-w-48 shrink-0 truncate rounded-md px-2.5 py-1 text-xs transition-colors {activeSheet ===
              sheet.name
                ? 'bg-primary font-medium text-primary-foreground'
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
              title={sheet.name}
              onclick={() => chooseSheet(sheet.name)}>{sheet.name}</button
            >
          {/each}
        </div>
        {#if sheetsOverflow}
          <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-8 bg-gradient-to-r from-transparent to-white dark:to-black" aria-hidden="true"></div>
        {/if}
        {#if sheetScrolled}
          <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-8 bg-gradient-to-r from-white to-transparent dark:from-black" aria-hidden="true"></div>
        {/if}
      </div>
      {#if sheetsOverflow}
        <div class="flex shrink-0 items-center bg-white dark:bg-black">
          <TabListButton
            items={sheetItems}
            onpick={chooseSheet}
            label="Workbook sheets"
            icon="lucideMenu"
          />
        </div>
      {/if}
      {#if selectedSheet}
        <span class="ml-auto shrink-0 pl-1 text-[11px] tabular-nums text-muted-foreground">
          {selectedSheet.rowCount} rows · {selectedSheet.columnCount} columns
        </span>
      {/if}
    </div>
  {/if}

  <div
    bind:this={scroll}
    class="relative min-h-0 min-w-0 flex-1 overflow-x-auto overflow-y-auto {format === 'xlsx' ? 'bg-white dark:bg-black' : 'bg-muted/30 p-4 md:p-6'}"
    onwheel={handleWheel}
  >
    {#if format === 'xlsx' && selectedSheet}
      <div
        class="w-max min-w-full bg-white dark:bg-black"
      >
        <table class="border-collapse text-left text-xs">
          <thead
            class="sticky top-0 z-10 bg-zinc-100 text-[11px] font-medium text-zinc-600 dark:bg-black dark:text-white"
          >
            <tr
              ><th
                  class="sticky left-0 z-20 w-12 border-b border-r border-border bg-zinc-100 px-2 py-2 text-right font-normal text-zinc-600 dark:bg-black dark:text-white"
                >#</th
              >{#each Array.from({ length: Math.min(selectedSheet.columnCount, 32) }, (_, index) => index) as column}<th
                  class="min-w-28 max-w-64 border-b border-r border-border px-3 py-2 text-center font-medium"
                  >{columnName(column)}</th
                >{/each}</tr
            >
          </thead>
          <tbody>
            {#each selectedSheet.rows as row, rowIndex}
              <tr class="group hover:bg-primary/[0.035]">
                <th
                  class="sticky left-0 z-[1] w-12 border-b border-r border-border bg-zinc-100 px-2 py-1.5 text-right font-mono text-[10px] font-normal text-zinc-600 dark:bg-black dark:text-white"
                  >{rowIndex + 1}</th
                >
                {#each Array.from({ length: Math.min(selectedSheet.columnCount, 32) }, (_, index) => index) as column}
                  <td
                    class="max-w-64 border-b border-r border-border/70 px-3 py-1.5 align-top text-zinc-950 dark:text-white"
                    ><span
                      class="block max-h-20 overflow-hidden whitespace-pre-wrap break-words"
                      >{row[column] ?? ''}</span
                    ></td
                  >
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
        {#if selectedSheet.truncated}<div
            class="border-t border-border bg-muted/50 px-3 py-2 text-[11px] text-muted-foreground"
          >
            Showing the first 250 rows and 32 columns for a smooth preview.
          </div>{/if}
      </div>
    {:else if format === 'xlsx' && !loading && !error}
      <div
        class="flex h-full min-h-40 items-center justify-center text-xs text-muted-foreground"
      >
        This workbook has no visible sheets.
      </div>
    {/if}
    <div
      bind:this={documentHost}
      class="office-document mx-auto min-h-full max-w-5xl"
      class:hidden={format === 'xlsx'}
      aria-label={label}
    ></div>
    {#if loading}
      <div
        class="absolute inset-0 z-20 flex items-center justify-center gap-2 bg-background/75 text-xs text-muted-foreground backdrop-blur-[2px]"
      >
        <ZIcon
          zType={icon('lucideLoaderCircle')}
          zSize="15"
          class="animate-spin"
        /> Preparing preview
      </div>
    {:else if error}
      <div
        class="absolute inset-0 z-20 flex items-center justify-center bg-background/75 p-6 backdrop-blur-[2px]"
      >
        <div
          class="flex max-w-md flex-col items-center gap-2 rounded-xl border border-border bg-background p-6 text-center shadow-sm"
        >
          <span
            class="grid size-10 place-items-center rounded-full bg-destructive/10 text-destructive"
            ><ZIcon zType={icon('lucideTriangleAlert')} zSize="19" /></span
          >
          <p class="m-0 text-sm font-medium text-foreground">
            Preview unavailable
          </p>
          <p class="m-0 text-xs leading-relaxed text-muted-foreground">
            {error}
          </p>
        </div>
      </div>
    {/if}
  </div>

  <div
    class="flex h-7 shrink-0 items-center gap-2 border-t border-border px-3 text-[11px] text-muted-foreground"
  >
    <span class="min-w-0 flex-1 truncate font-mono" title={shortPath}
      >{shortPath}</span
    >
    {#if format === 'xlsx' && workbook}<span class="shrink-0"
        >{workbook.sheets.length} sheets</span
      >{/if}
    <span class="shrink-0">Preview</span>
  </div>
</div>

<style>
  .office-document :global(.docx-wrapper) {
    min-height: 100%;
    padding: 1.5rem 0 3rem;
    background: transparent;
  }

  .office-document :global(.docx-wrapper > section.docx) {
    margin: 0 auto 1.5rem;
    max-width: 100%;
    background-color: #fff !important;
    color: #111827 !important;
    box-shadow:
      0 1px 3px rgb(15 23 42 / 8%),
      0 10px 30px rgb(15 23 42 / 8%);
  }

  :global(.dark .office-document .docx-wrapper > section.docx) {
    background-color: #000 !important;
    color: #fff !important;
  }

  .office-document :global(.pptx-slide) {
    margin: 0 auto 1.75rem;
    overflow: hidden;
    border-radius: 0.35rem;
    box-shadow:
      0 2px 5px rgb(15 23 42 / 9%),
      0 12px 32px rgb(15 23 42 / 12%);
  }
</style>
