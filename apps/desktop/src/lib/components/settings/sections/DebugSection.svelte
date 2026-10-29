<script lang="ts">
  import { fade } from 'svelte/transition';
  import { ZButton, ZIcon, ZInput } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { debugLogFolder, saveDebugBundle } from '../../../tasks/flow-log';
  import { toast } from '../../../app/platform/toast';

  const icon = (name: string) => name as ZIconName;

  let saving = $state(false);
  let folder = $state('');
  void debugLogFolder().then((path) => (folder = path));

  async function copyFolder() {
    try {
      await navigator.clipboard.writeText(folder);
      toast.success('Folder path copied');
    } catch (error) {
      toast.error(`Could not copy the folder path: ${String(error)}`);
    }
  }

  async function save() {
    saving = true;
    try {
      const path = await saveDebugBundle();
      if (path) toast.success(`Debug log saved: ${path}`);
    } catch (error) {
      toast.error(`Could not save the debug log: ${String(error)}`);
    } finally {
      saving = false;
    }
  }
</script>

<div class="flex flex-col gap-4" in:fade={{ duration: 200 }}>
  <section class="flex items-start gap-3 rounded-xl border border-border bg-card/40 p-4">
    <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/12 text-primary"><ZIcon zType={icon('lucideBug')} class="size-4.5" /></span>
    <div class="min-w-0 flex-1">
      <h3 class="text-sm font-semibold">Debug log is always on</h3>
      <p class="mt-1 text-[0.75rem] text-muted-foreground">
        The page's memory and page size, every step that takes 50 ms or more, the batches of streamed events, and the step
        it was on when it stopped answering are written to the log all the time. Nothing you type or the agent answers is
        written. Lines older than 2 days are removed.
      </p>
    </div>
  </section>

  <section class="rounded-xl border border-border p-4">
    <div class="text-sm font-medium">Debug log</div>
    <p class="mt-0.5 text-[0.75rem] text-muted-foreground">
      Written to <code>debug.log</code> (with <code>debug.1.log</code> before it) and <code>errors.log</code> in the folder below.
      Read it with the heartbeat lines: <code>page silent</code> means the page stopped writing; <code>ui blocked</code> names
      the step it was on.
    </p>
    <p class="mt-2 text-[0.75rem] text-muted-foreground">Default folder</p>
    <div class="mt-1">
      <ZInput value={folder} zReadonly class="font-mono" aria-label="Default folder">
        {#snippet suffix()}
          <span class="flex shrink-0 items-center gap-0.5">
            <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0" aria-label="Copy folder path" title="Copy path" disabled={!folder} onclick={() => void copyFolder()}>
              <ZIcon zType={icon('lucideCopy')} class="size-3.5" />
            </ZButton>
            <ZButton zWave={false} zType="ghost" zSize="sm" class="size-6! p-0" aria-label="Save a copy to" title="Save a copy to…" disabled={saving} onclick={() => void save()}>
              <ZIcon zType={icon('lucideDownload')} class="size-3.5" />
            </ZButton>
          </span>
        {/snippet}
      </ZInput>
    </div>
    <p class="mt-1.5 text-[0.75rem] text-muted-foreground">Save a copy: errors.log and debug.log together in one file, for a bug report.</p>
  </section>
</div>
