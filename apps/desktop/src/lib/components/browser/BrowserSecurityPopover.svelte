<script lang="ts">
  import { ZPopover } from '@shival99/z-ui-svelte';
  import type { SiteSecurity } from './browser-helpers';

  let { anchor, open = $bindable(false), security }: { anchor: HTMLElement | null; open: boolean; security: SiteSecurity | null } = $props();
</script>

<ZPopover {anchor} bind:open zTrigger="manual" zPosition="bottom-left" zOffset={6} zWidth={280}>
  {#snippet children()}
    {#if security}
      <div class="grid gap-2 p-3 text-xs">
        <div class="flex items-center gap-2 font-medium {security.kind === 'secure' ? 'text-emerald-600 dark:text-emerald-400' : security.kind === 'insecure' ? 'text-amber-600 dark:text-amber-400' : ''}">
          {security.kind === 'secure' ? 'Connection is secure' : security.kind === 'local' ? 'Local address (not encrypted)' : 'Connection is not secure'}
        </div>
        <p class="m-0 text-muted-foreground">
          {security.kind === 'secure' ? 'Data sent to this site is encrypted.' : security.kind === 'local' ? 'This page runs on your own machine, so nothing leaves it.' : 'Anything you send to this site can be read by others. Do not enter passwords or card numbers.'}
        </p>
        <dl class="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1">
          <dt class="text-muted-foreground">Protocol</dt><dd class="m-0">{security.protocol}</dd>
          <dt class="text-muted-foreground">Host</dt><dd class="m-0 truncate" title={security.host}>{security.host}</dd>
          <dt class="text-muted-foreground">Port</dt><dd class="m-0">{security.port}</dd>
        </dl>
        {#if security.kind === 'secure'}<p class="m-0 text-[11px] text-muted-foreground">Certificate details are not available in the embedded browser.</p>{/if}
      </div>
    {/if}
  {/snippet}
</ZPopover>
