<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZDropdownMenuItem, ZIconName } from '@shival99/z-ui-svelte';
  import { onDestroy, untrack } from 'svelte';
  import { fileSize, readFileBytes, readImageDataUrl, revealFile } from '../agent';
  import { clock, findArtifacts, type Artifact } from '../artifacts';
  import { fileKind, fileSizeLabel } from '../attachments';
  import { dirOf } from '../paths';
  import { openAttachment, openMedia } from '../preview.svelte';
  import { toast } from '../toast';
  import { inTauri } from '../history';
  import FileChip from './FileChip.svelte';
  import MenuButton from './MenuButton.svelte';
  import type { Block } from '../mock';

  // The files an agent made or named in a reply, drawn as cards under it: a sound plays right there, a video too, a
  // picture opens large, a document opens in the viewer. It reads the reply's words and Write calls (artifacts.ts), so
  // it is the same for every agent and provider. A file the reply names that does not exist yet (the agent is still
  // making it) shows as a card that shimmers, and turns into the real one when the file appears.
  let { blocks, root, live = false }: { blocks: Block[]; root: string | null; live?: boolean } = $props();

  const found = $derived(findArtifacts(blocks as { kind: string; text?: string; command?: string }[], root));

  /** Size of each file that exists (bytes); a path missing from it does not exist (yet). */
  let sizes = $state<Record<string, number>>({});
  /** The paths already asked about, so a finished reply does not ask again. */
  const asked = new Set<string>();
  let poll: ReturnType<typeof setInterval> | undefined;

  async function look(path: string) {
    try {
      const size = await fileSize(path);
      if (size !== null) sizes[path] = size;
    } catch {
      // Not there (yet).
    }
  }

  $effect(() => {
    const list = found;
    if (!inTauri()) return;
    untrack(() => {
      for (const item of list) if (!asked.has(item.path)) { asked.add(item.path); void look(item.path); }
    });
  });
  // While the reply runs, the files it named that are not there yet are looked for again until they appear.
  $effect(() => {
    clearInterval(poll);
    if (!live || !inTauri()) return;
    poll = setInterval(() => {
      for (const item of found) if (sizes[item.path] === undefined) void look(item.path);
    }, 1200);
    return () => clearInterval(poll);
  });
  onDestroy(() => clearInterval(poll));

  const ready = $derived(found.filter((item) => sizes[item.path] !== undefined));
  const pending = $derived(live ? found.filter((item) => sizes[item.path] === undefined) : []);
  const images = $derived(ready.filter((item) => item.kind === 'image'));
  const others = $derived(ready.filter((item) => item.kind !== 'image'));

  // ---------- pictures ----------
  let pictures = $state<Record<string, string>>({});
  $effect(() => {
    for (const item of images) {
      if (pictures[item.path] === undefined) {
        pictures[item.path] = '';
        void readImageDataUrl(item.path).then((url) => (pictures[item.path] = url)).catch(() => (pictures[item.path] = 'failed'));
      }
    }
  });
  const shownPictures = $derived(images.filter((item) => pictures[item.path] && pictures[item.path] !== 'failed'));
  // Pictures and videos of the reply open in one gallery (z-ui's lightbox), side by side with arrows.
  const videos = $derived(others.filter((item) => item.kind === 'video'));
  async function openVisual(item: Artifact) {
    if (item.kind === 'video') await load(item);
    const list = ready.filter((entry) => (entry.kind === 'image' && pictures[entry.path] && pictures[entry.path] !== 'failed') || (entry.kind === 'video' && blobs[entry.path]));
    const at = Math.max(0, list.findIndex((entry) => entry.path === item.path));
    openMedia(
      list.map((entry) =>
        entry.kind === 'image'
          ? { name: entry.name, url: pictures[entry.path], thumb: pictures[entry.path], type: pictures[entry.path].slice(5, pictures[entry.path].indexOf(';')) || 'image/png', size: sizes[entry.path] }
          : { name: entry.name, url: blobs[entry.path], type: mimeOf(entry.name) || 'video/mp4', size: sizes[entry.path] },
      ),
      at,
    );
  }
  const openPicture = (item: Artifact) => void openVisual(item);

  // ---------- sound and video ----------
  const MIME: Record<string, string> = { wav: 'audio/wav', mp3: 'audio/mpeg', ogg: 'audio/ogg', oga: 'audio/ogg', m4a: 'audio/mp4', flac: 'audio/flac', aac: 'audio/aac', opus: 'audio/ogg', weba: 'audio/webm', mp4: 'video/mp4', m4v: 'video/mp4', webm: 'video/webm', mov: 'video/quicktime', mkv: 'video/x-matroska', avi: 'video/x-msvideo' };
  const mimeOf = (name: string) => MIME[name.split('.').pop()?.toLowerCase() ?? ''] ?? '';
  const blobs = $state<Record<string, string>>({});
  const loading = new Set<string>();
  async function load(item: Artifact) {
    if (blobs[item.path] || loading.has(item.path)) return;
    loading.add(item.path);
    try {
      blobs[item.path] = URL.createObjectURL(new Blob([await readFileBytes(item.path)], { type: mimeOf(item.name) }));
    } catch {
      toast.error(`Could not open ${item.name}`);
    } finally {
      loading.delete(item.path);
    }
  }
  // A sound or short video is read at once (its length and the player are then ready); a big one waits for the click.
  $effect(() => {
    for (const item of others) if ((item.kind === 'audio' || item.kind === 'video') && (sizes[item.path] ?? 0) <= 20 * 1024 * 1024) void load(item);
  });
  onDestroy(() => Object.values(blobs).forEach((url) => URL.revokeObjectURL(url)));

  type PlayState = { playing: boolean; time: number; length: number };
  let play = $state<Record<string, PlayState>>({});
  const audios: Record<string, HTMLAudioElement> = {};
  const stateOf = (path: string): PlayState => play[path] ?? { playing: false, time: 0, length: 0 };
  function bind(node: HTMLAudioElement, path: string) {
    audios[path] = node;
    const sync = () => (play[path] = { playing: !node.paused && !node.ended, time: node.currentTime, length: Number.isFinite(node.duration) ? node.duration : 0 });
    for (const name of ['play', 'pause', 'ended', 'timeupdate', 'loadedmetadata', 'durationchange']) node.addEventListener(name, sync);
    return { destroy() { delete audios[path]; } };
  }
  async function toggle(item: Artifact) {
    await load(item);
    await new Promise((resolve) => setTimeout(resolve, 0));
    const node = audios[item.path];
    if (!node) return;
    // One sound at a time.
    for (const [path, other] of Object.entries(audios)) if (path !== item.path) other.pause();
    if (node.paused) void node.play().catch(() => toast.error('Could not play the sound'));
    else node.pause();
  }
  function seek(item: Artifact, event: Event) {
    const node = audios[item.path];
    if (node) node.currentTime = Number((event.currentTarget as HTMLInputElement).value);
  }

  // ---------- menu ----------
  async function saveAs(item: Artifact) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const saved = await invoke<string | null>('file_copy_as', { path: item.path });
      if (saved) toast.success(`Saved ${saved.split(/[\\/]/).pop()}`);
    } catch (error) {
      toast.error(`Could not save: ${error}`);
    }
  }
  const menuOf = (item: Artifact): ZDropdownMenuItem[] => [
    { label: 'Save as…', icon: 'lucideDownload', iconSize: '12', class: 'text-xs! py-1', onClick: () => void saveAs(item) },
    { label: 'Show in folder', icon: 'lucideFolderOpen', iconSize: '12', class: 'text-xs! py-1', onClick: () => void revealFile(item.path).catch(() => toast.error('Could not show the file')) },
  ];
  const typeLabel = (item: Artifact) => `${fileKind(item.name).label} ${item.kind === 'audio' ? 'audio' : item.kind === 'video' ? 'video' : ''}`.trim();
</script>

{#if ready.length > 0 || pending.length > 0}
  <div class="flex w-full flex-col gap-2" aria-label="Files of this reply">
    {#each pending as item (item.path)}
      <div class="artifact-pending relative flex h-[60px] w-full max-w-xl items-center gap-3 overflow-hidden rounded-xl border border-border bg-muted/30 px-3" role="status" aria-label="Creating {item.name}">
        <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={(item.kind === 'audio' ? 'lucideMusic' : item.kind === 'video' ? 'lucideFilm' : item.kind === 'image' ? 'lucideImage' : 'lucideFile') as ZIconName} class="size-4.5 animate-pulse" /></span>
        <span class="flex min-w-0 flex-col leading-tight">
          <span class="truncate text-sm font-medium text-foreground">{item.name}</span>
          <span class="mt-0.5 text-[11px] text-muted-foreground">Creating…</span>
        </span>
      </div>
    {/each}

    {#if images.length > 0 || videos.length > 0}
      <!-- Pictures and videos share one row of small thumbnails. -->
      <div class="flex flex-wrap gap-2">
        {#each images as item (item.path)}
          {#if pictures[item.path] && pictures[item.path] !== 'failed'}
            <div class="group/pic relative">
              <button type="button" class="artifact-in block cursor-zoom-in overflow-hidden rounded-xl border border-border bg-muted/30 outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label="Open {item.name}" onclick={() => openPicture(item)}>
                <img src={pictures[item.path]} alt={item.name} class="block h-36 w-auto max-w-64 object-cover" />
              </button>
              <span class="absolute top-1.5 right-1.5 opacity-0 transition-opacity group-hover/pic:opacity-100 focus-within:opacity-100"><MenuButton items={menuOf(item)} label="Actions for {item.name}" class="size-7! rounded-md! bg-background/80! backdrop-blur" /></span>
            </div>
          {:else if pictures[item.path] === ''}
            <div class="artifact-pending relative h-36 w-52 overflow-hidden rounded-xl border border-border bg-muted/30" aria-hidden="true"></div>
          {:else}
            <FileChip name={item.name} size={sizes[item.path]} onclick={() => void openAttachment(item.path, item.name, sizes[item.path])} />
          {/if}
        {/each}
        {#each videos as item (item.path)}
          <div class="group/pic relative">
            <button type="button" class="artifact-in relative block cursor-pointer overflow-hidden rounded-xl border border-border bg-black outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label="Play {item.name}" onclick={() => void openVisual(item)}>
              {#if blobs[item.path]}
                <!-- The first frame is the thumbnail: `#t=0.1` makes the engine decode it without playing. -->
                <video src="{blobs[item.path]}#t=0.1" preload="metadata" muted playsinline class="pointer-events-none block h-36 w-auto max-w-64 object-cover"></video>
              {:else}
                <span class="block h-36 w-64"></span>
              {/if}
              <span class="absolute inset-0 grid place-items-center">
                <span class="grid size-9 place-items-center rounded-full bg-black/55 text-white backdrop-blur-sm transition group-hover/pic:scale-105 group-hover/pic:bg-black/70"><ZIcon zType={'lucidePlay' as ZIconName} class="size-4" /></span>
              </span>
              <span class="absolute bottom-1 left-1 max-w-[calc(100%-0.5rem)] truncate rounded bg-black/60 px-1.5 py-0.5 text-[10px] text-white" title={item.path}>{item.name} · {fileSizeLabel(sizes[item.path])}</span>
            </button>
            <span class="absolute top-1.5 right-1.5 opacity-0 transition-opacity group-hover/pic:opacity-100 focus-within:opacity-100"><MenuButton items={menuOf(item)} label="Actions for {item.name}" class="size-7! rounded-md! bg-background/80! backdrop-blur" /></span>
          </div>
        {/each}
      </div>
    {/if}

    {#each others as item (item.path)}
      {#if item.kind === 'audio'}
        {@const state = stateOf(item.path)}
        <div class="artifact-in flex w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-muted/30">
          <div class="flex items-center gap-3 px-3 py-2.5">
            <span class="grid size-11 shrink-0 place-items-center rounded-lg bg-background text-primary"><ZIcon zType={'lucideAudioLines' as ZIconName} class="size-5 {state.playing ? 'animate-pulse' : ''}" /></span>
            <span class="flex min-w-0 flex-1 flex-col leading-tight">
              <span class="truncate text-sm font-semibold text-foreground" title={item.path}>{item.name}</span>
              <span class="mt-0.5 truncate text-xs text-muted-foreground">{typeLabel(item)} · {fileSizeLabel(sizes[item.path])}</span>
            </span>
            <button type="button" class="grid size-9 shrink-0 cursor-pointer place-items-center rounded-full bg-background text-foreground outline-none transition hover:bg-primary/15 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring" aria-label={state.playing ? `Pause ${item.name}` : `Play ${item.name}`} onclick={() => void toggle(item)}>
              <ZIcon zType={(state.playing ? 'lucidePause' : 'lucidePlay') as ZIconName} class="size-4" />
            </button>
            <MenuButton items={menuOf(item)} label="Actions for {item.name}" />
          </div>
          <div class="flex items-center gap-3 border-t border-border px-3 py-2">
            <input type="range" min="0" max={state.length || 1} step="0.01" value={state.time} disabled={!blobs[item.path]} class="artifact-range min-w-0 flex-1" style:--fill="{state.length ? (state.time / state.length) * 100 : 0}%" aria-label="Position in {item.name}" oninput={(event) => seek(item, event)} />
            <span class="shrink-0 text-xs tabular-nums text-muted-foreground">{clock(state.time)} / {clock(state.length)}</span>
          </div>
          {#if blobs[item.path]}<audio use:bind={item.path} src={blobs[item.path]} preload="metadata" class="hidden"></audio>{/if}
        </div>
      {:else if item.kind === 'video'}
        <!-- Videos are drawn above, as thumbnails (see `videos`). -->
      {:else}
        <div class="artifact-in flex flex-wrap items-center gap-1.5">
          <FileChip name={item.name} size={sizes[item.path]} onclick={() => void openAttachment(item.path, item.name, sizes[item.path])} />
          <MenuButton items={menuOf(item)} label="Actions for {item.name}" />
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  /* A card appears with a short rise, so a file that just finished does not pop in. */
  .artifact-in { animation: artifact-in 0.28s cubic-bezier(0.2, 0.8, 0.2, 1) both; }
  @keyframes artifact-in { from { opacity: 0; transform: translateY(6px) scale(0.98); } to { opacity: 1; transform: none; } }

  /* A file still being made: a soft light sweeping across the card. */
  .artifact-pending::after {
    content: '';
    position: absolute;
    inset: 0;
    transform: translateX(-100%);
    background: linear-gradient(90deg, transparent, color-mix(in oklab, var(--primary) 14%, transparent), transparent);
    animation: artifact-sweep 1.4s ease-in-out infinite;
  }
  @keyframes artifact-sweep { to { transform: translateX(100%); } }

  /* The position slider: a thin track filled up to the playhead, with a round handle. */
  .artifact-range { -webkit-appearance: none; appearance: none; height: 4px; border-radius: 9999px; cursor: pointer; background: linear-gradient(to right, var(--primary) var(--fill, 0%), color-mix(in oklab, var(--foreground) 14%, transparent) var(--fill, 0%)); outline: none; }
  .artifact-range:disabled { cursor: default; opacity: 0.5; }
  .artifact-range::-webkit-slider-thumb { -webkit-appearance: none; appearance: none; width: 12px; height: 12px; border-radius: 9999px; background: var(--primary); border: 2px solid var(--background); box-shadow: 0 0 0 1px color-mix(in oklab, var(--primary) 50%, transparent); }
  @media (prefers-reduced-motion: reduce) { .artifact-in, .artifact-pending::after { animation: none; } }
</style>
