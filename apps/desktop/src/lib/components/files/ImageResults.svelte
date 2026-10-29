<script lang="ts">
  import { ZIcon } from '@shival99/z-ui-svelte';
  import { activeSession, draftImages, type Attachment } from '../../chat/sessions.svelte';
  import { revealFile } from '../../chat/agent';
  import { openImages } from '../../browser/preview.svelte';
  import { gen, type Preview } from '../../files/image-gen.svelte';
  import { modelLabel } from '../../agents/models.svelte';
  import { toast } from '../../app/platform/toast';
  import DevEmpty from '../browser/DevEmpty.svelte';
  import ImageDeveloping from './ImageDeveloping.svelte';

  const working = $derived(gen.working);
  const previews = $derived<Preview[]>(gen.previews);
  const failureLines = $derived(gen.failureLines);
  const runMs = $derived(gen.runMs);
  const loadingRun = $derived(gen.loadingRun);
  const runSkeleton = $derived(gen.runSkeleton);
  let failuresOpen = $state(false);

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
</script>

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
