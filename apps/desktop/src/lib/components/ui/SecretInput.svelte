<script lang="ts">
  import { ZButton, ZIcon, ZInput } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import { maskSecret } from '../../app/text/secret-mask';

  // A key field that never shows a saved key again: once a key is saved it reads as `sk-ant…••••1a2b`. Replace opens
  // an empty field for a new one; left empty, the saved key stays. A key is seen only while it is first typed.
  let {
    value = $bindable(''),
    kept = '',
    placeholder = 'sk-…',
  }: {
    value?: string;
    /** The key saved before the form opened; empty for a new one. */
    kept?: string;
    placeholder?: string;
  } = $props();

  let replacing = $state(false);
  let typed = $state('');
  const showingKept = $derived(kept !== '' && !replacing);

  // What the form saves: what was typed, else the saved key.
  $effect(() => {
    const next = kept && !typed.trim() ? kept : typed;
    if (value !== next) value = next;
  });
</script>

{#if showingKept}
  <div class="flex h-9 min-w-0 items-center gap-1 rounded-md border border-border bg-muted/40 pr-1 pl-3">
    <ZIcon zType={'lucideKeyRound' as ZIconName} class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground select-none" aria-label="Saved key, hidden">{maskSecret(kept)}</span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 shrink-0 px-2 text-xs" onclick={() => (replacing = true)}>Replace</ZButton>
  </div>
{:else}
  <ZInput zType="password" autocomplete="off" bind:value={typed} zPlaceholder={kept ? 'Paste the new key' : placeholder} />
{/if}
