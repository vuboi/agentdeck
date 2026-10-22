<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { Block } from '../mock';

  // The agent wants to run something it was not allowed to: say what, and let the user decide.
  let { block, onanswer }: { block: Extract<Block, { kind: 'permission' }>; onanswer?: (answer: 'once' | 'always' | 'deny') => void } = $props();

  const verb = (tool: string) =>
    /^(Edit|MultiEdit|Write|NotebookEdit)$/.test(tool) ? 'edit a file' : tool === 'Bash' ? 'run a command' : `use ${tool}`;
  const wants = $derived(block.requests.length === 1 ? `Claude wants to ${verb(block.requests[0].tool)}` : `Claude wants to do ${block.requests.length} things`);
</script>

<div class="block-in overflow-hidden rounded-xl border {block.state === 'pending' ? 'border-warning/50 bg-warning/5' : 'border-border bg-muted/20'} text-xs" role="group" aria-label="Permission request">
  <div class="flex items-center gap-2 px-3 py-2 font-medium">
    <ZIcon zType={block.state === 'denied' ? 'lucideShieldX' : block.state === 'allowed' ? 'lucideShieldCheck' : 'lucideShieldAlert'} class="size-4 {block.state === 'pending' ? 'text-warning' : 'text-muted-foreground'}" />
    <span>{block.state === 'pending' ? wants : block.state === 'allowed' ? 'Allowed' : 'Denied'}</span>
  </div>
  <div class="flex flex-col gap-1 px-3 pb-2">
    {#each block.requests as request (request.id)}
      <pre class="m-0 overflow-x-auto rounded-md border border-border bg-background px-3 py-1.5 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all"><span class="text-muted-foreground">{request.tool}</span>  {request.summary}</pre>
    {/each}
  </div>
  {#if block.state === 'pending'}
    <div class="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-2">
      <ZButton zWave={false} zType="default" zSize="sm" class="h-7 px-3 text-xs" onclick={() => onanswer?.('once')}>Allow</ZButton>
      <ZButton zWave={false} zType="outline" zSize="sm" class="h-7 px-3 text-xs" onclick={() => onanswer?.('always')} title="Allow this in the rest of the chat">Always allow</ZButton>
      <ZButton zWave={false} zType="secondary" zSize="sm" class="ml-auto h-7 px-3 text-xs" onclick={() => onanswer?.('deny')}>Deny</ZButton>
    </div>
  {/if}
</div>
