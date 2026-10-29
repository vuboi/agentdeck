<script lang="ts">
  import type { Snippet } from 'svelte';

  /** Shared 40px Store toolbar: fits 32px controls and includes its bottom divider. */
  let { children }: { children: Snippet } = $props();
</script>

<div class="@container flex h-10 min-h-10 min-w-0 shrink-0 items-center gap-1 border-b border-border px-1.5 text-sm">
  {@render children()}
</div>
