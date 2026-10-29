<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZAudioPlayer, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { readFileBytes, revealFile } from '../../chat/agent';
  import { mediaOf } from '../../files/files';
  import { inTauri } from '../../app/platform/history';
  import { displayPath } from '../../files/paths';
  import LoadingBar from '../ui/LoadingBar.svelte';

  // A sound or a video opened in the file viewer: played, not shown as text. The file is read once into a blob URL
  // (it can be anywhere on the disk: an agent's output folder, not only the project).
  let { path, root = '', name, size }: { path: string; root?: string; name: string; size?: string } = $props();
  const shortPath = $derived(displayPath(root, path).replace(/\\/g, '/'));

  const media = $derived(mediaOf(name));
  let url = $state('');
  let error = $state('');
  let bytes = $state(0);

  $effect(() => {
    const kind = media;
    const target = path;
    url = '';
    error = '';
    if (!kind) return;
    if (!inTauri()) {
      error = 'Open the desktop app to play files.';
      return;
    }
    let cancelled = false;
    let made = '';
    readFileBytes(target)
      .then((buffer) => {
        if (cancelled) return;
        bytes = buffer.byteLength;
        made = URL.createObjectURL(new Blob([buffer], { type: kind.mime }));
        url = made;
      })
      .catch((failure: unknown) => {
        if (!cancelled) error = String(failure).includes('large') ? 'This file is larger than 50 MB.' : `Could not read ${name}.`;
      });
    return () => {
      cancelled = true;
      if (made) URL.revokeObjectURL(made);
    };
  });

  const sizeLabel = $derived(size ?? (bytes ? (bytes > 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`) : ''));
  const ext = $derived(name.split('.').pop()?.toUpperCase() ?? '');
</script>

<div class="relative flex min-h-0 flex-1 flex-col bg-background">
  <div class="h-10 shrink-0 border-b border-border"></div>
  <div class="relative flex min-h-0 flex-1 items-center justify-center overflow-auto p-6">
    {#if error}
      <p class="max-w-sm text-center text-sm text-destructive" role="alert">{error}</p>
    {:else if !url}
      <LoadingBar label="Loading {name}" />
    {:else if media?.kind === 'audio'}
      <!-- A card: the file's sound in the middle of the pane, with its waveform and the usual controls. -->
      <div class="w-full max-w-xl rounded-2xl border border-border bg-card/60 p-5 shadow-sm" in:fade={{ duration: 180 }}>
        <div class="mb-4 flex items-center gap-3">
          <span class="grid size-12 shrink-0 place-items-center rounded-xl bg-primary/12 text-primary"><ZIcon zType={'lucideAudioLines' as ZIconName} class="size-6" /></span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm font-semibold" title={name}>{name}</span>
            <span class="block text-sm text-muted-foreground">{ext} audio{sizeLabel ? ` · ${sizeLabel}` : ''}</span>
          </span>
          <button type="button" class="grid size-8 shrink-0 cursor-pointer place-items-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground" title="Show in folder" aria-label="Show {name} in its folder" onclick={() => void revealFile(path)}>
            <ZIcon zType={'lucideFolderOpen' as ZIconName} class="size-4" />
          </button>
        </div>
        <ZAudioPlayer src={url} title={name} zWaveform="real" zSkip zSpeed zVolume zExclusive />
      </div>
    {:else}
      <!-- svelte-ignore a11y_media_has_caption (a file from the disk: there is no caption track to give) -->
      <video src={url} controls class="max-h-full max-w-full rounded-xl bg-black shadow-lg" in:fade={{ duration: 180 }}></video>
    {/if}
  </div>
  <div class="flex h-7 shrink-0 items-center gap-3 border-t border-border px-3 text-[0.6875rem] text-muted-foreground">
    <span class="min-w-0 flex-1 truncate font-mono" title={shortPath}>{shortPath}</span>
    {#if sizeLabel}<span class="shrink-0 whitespace-nowrap tabular-nums">{ext} · {sizeLabel}</span>{/if}
  </div>
</div>
