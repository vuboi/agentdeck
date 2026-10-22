<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';

  let maximized = $state(false);
  const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  onMount(() => {
    if (!inTauri) return;
    const current = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void current.isMaximized().then((value) => {
      if (!disposed) maximized = value;
    });
    void current.onResized(() => {
      void current.isMaximized().then((value) => {
        if (!disposed) maximized = value;
      });
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  async function windowAction(action: 'minimize' | 'toggle' | 'close') {
    if (!inTauri) return;
    const current = getCurrentWindow();
    if (action === 'minimize') await current.minimize();
    if (action === 'close') await current.close();
    if (action === 'toggle') {
      await current.toggleMaximize();
      maximized = await current.isMaximized();
    }
  }
</script>

<!-- Pinned to the top-right corner, over the header row of whichever pane sits there. -->
<div class="absolute top-0 right-0 z-50 flex h-[39px] items-center gap-0.5 pr-[5.5px]" aria-label="Window controls">
  <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-8! items-center justify-center rounded-md !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label="Minimize window" onclick={() => windowAction('minimize')}>
    <ZIcon zType="lucideMinus" class="size-4" zStrokeWidth={1.5} />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-8! items-center justify-center rounded-md !bg-transparent !text-muted-foreground hover:!bg-primary/10 hover:!text-primary" aria-label={maximized ? 'Restore window' : 'Maximize window'} onclick={() => windowAction('toggle')}>
    <ZIcon zType={maximized ? "lucideCopy" : "lucideSquare"} class="size-3.5" zStrokeWidth={1.5} />
  </ZButton>
  <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-8! items-center justify-center rounded-md !bg-transparent !text-muted-foreground hover:!bg-destructive hover:!text-white" aria-label="Close window" onclick={() => windowAction('close')}>
    <ZIcon zType="lucideX" class="size-4" zStrokeWidth={1.5} />
  </ZButton>
</div>
