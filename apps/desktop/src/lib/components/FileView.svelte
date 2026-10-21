<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ContentTab } from '../content-tabs.svelte';
  import { languageOf, readProjectFile } from '../files';
  import CodeView from './CodeView.svelte';

  let { file }: { file: Extract<ContentTab, { kind: 'file' }> } = $props();

  // Text is null until read: the area stays empty for that moment, with no spinner.
  let text = $state<string | null>(null);
  let error = $state('');

  $effect(() => {
    const target = file;
    text = null;
    error = '';
    readProjectFile(target.root, target.path)
      .then((content) => {
        if (file === target) text = content;
      })
      .catch((failure: unknown) => {
        if (file === target) error = String(failure);
      });
  });

  const language = $derived(languageOf(file.name));
</script>

<div class="flex h-full min-h-0 flex-col bg-background" role="tabpanel" aria-label={file.name}>
  {#if error}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
      <ZIcon zType="lucideFile" class="size-8 opacity-60" />
      <p class="m-0 max-w-md break-all font-mono text-xs leading-relaxed">{file.path}</p>
      <p class="m-0 max-w-md text-xs leading-relaxed text-destructive">{error}</p>
    </div>
  {:else if text !== null}
    <CodeView text={text} lang={language} />
  {/if}
</div>
