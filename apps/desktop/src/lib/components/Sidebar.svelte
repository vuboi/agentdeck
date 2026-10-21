<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { sessions } from '../mock';

  let selected = $state('s1');

  const statusColor: Record<string, string> = {
    working: 'bg-success',
    waiting: 'bg-warning',
    idle: 'bg-muted-foreground/40',
  };

  function agentLabel(agent: string) { return agent[0].toUpperCase() + agent.slice(1); }
</script>

<aside class="flex h-full min-w-0 flex-col bg-sidebar" aria-label="Sessions">
  <!-- Header row shared with the other panes: same height, one continuous bottom line. -->
  <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3" data-tauri-drag-region>
    <span class="grid size-5 place-items-center rounded-md bg-primary text-[11px] font-bold text-primary-foreground" data-tauri-drag-region>A</span>
    <span class="text-xs font-semibold" data-tauri-drag-region>AgentDeck</span>
  </div>

  <div class="px-2 pt-2">
    <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7 w-full justify-start gap-2 px-2 text-xs" onclick={() => (selected = '')}>
      <ZIcon zType="lucidePlus" class="size-3.5" />
      New session
    </ZButton>
  </div>

  <div class="px-4 pt-3 pb-1.5 font-mono text-[10px] tracking-widest text-muted-foreground">RECENT</div>
  <ul class="m-0 flex min-h-0 flex-1 list-none flex-col gap-px overflow-y-auto px-2 pb-2 p-0">
    {#each sessions as session (session.id)}
      <li>
        <ZButton
          zWave={false}
          zType="ghost"
          zSize="sm"
          class="h-auto w-full justify-start gap-2 px-2 py-1.5 text-left text-xs {selected === session.id ? 'bg-primary/15 text-primary hover:bg-primary/20' : ''}"
          aria-current={selected === session.id ? 'page' : undefined}
          onclick={() => (selected = session.id)}
        >
          <span class="size-1.5 shrink-0 rounded-full {statusColor[session.status]}" aria-label={session.status}></span>
          <span class="flex min-w-0 flex-1 items-baseline gap-2">
            <span class="truncate {selected === session.id ? 'font-medium text-primary' : 'text-foreground'}">{session.title}</span>
            <span class="shrink-0 text-[10px] text-muted-foreground">{agentLabel(session.agent)}</span>
          </span>
        </ZButton>
      </li>
    {/each}
  </ul>

  <div class="flex h-11 shrink-0 items-center gap-2 px-3 text-xs">
    <span class="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[10px]">JD</span>
    <span class="truncate">Jane Developer</span>
    <ZButton zWave={false} zType="ghost" zSize="sm" class="ml-auto h-6 w-6" aria-label="Account menu">
      <ZIcon zType="lucideEllipsis" class="size-3.5" />
    </ZButton>
  </div>
</aside>
