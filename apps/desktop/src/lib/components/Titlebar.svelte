<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

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

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region><span class="brand-mark">A</span> AgentDeck</div>
  <div class="window-actions" aria-label="Window controls">
    <button aria-label="Minimize window" onclick={() => windowAction('minimize')}>−</button>
    <button aria-label={maximized ? 'Restore window' : 'Maximize window'} onclick={() => windowAction('toggle')}>{maximized ? '❐' : '□'}</button>
    <button class="close" aria-label="Close window" onclick={() => windowAction('close')}>×</button>
  </div>
</header>
