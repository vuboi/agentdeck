<script lang="ts">
  import { untrack } from 'svelte';
  import { refreshOutput, watchServerLog, watchTerminalOutput, type Activity } from '../../chat/runs/activity.svelte';
  import { stopServers } from '../../browser/browser.svelte';
  import { stopTerminalRun } from '../../terminal/terminal.svelte';
  import { ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';

  // The card of one run / terminal / shell command in the Tasks panel (it exists only while the entry is open). A
  // Terminal entry is fed by its tab: the terminal pushes the new output as the shell prints it. A Run project entry
  // reads the server's log again when the Rust side says it grew (`server-log`), and once a second as a fallback, while
  // the server runs. A finished one keeps the last text and nothing runs for it.
  let { item, oncopy }: { item: Activity; oncopy: (text: string) => void } = $props();

  let alive = $state(false);
  const live = $derived(item.name === 'Run project' ? alive : item.status === 'running' || item.status === 'background');
  // The command line only: a run's input also names its folder on a second line.
  const command = $derived(item.name === 'Run project' ? item.input.split('\n')[0] : item.input);
  // What the one copy button puts on the clipboard: the command, then the output.
  const copied = $derived([command, item.output].filter(Boolean).join('\n'));

  $effect(() => {
    if (item.name !== 'Terminal') return;
    return watchTerminalOutput(item);
  });

  /** Throttle of the pushes, and the fallback poll. */
  const PUSH = 100;
  const FALLBACK = 1000;

  $effect(() => {
    if (item.name !== 'Run project') return;
    let current = true;
    let busy = false;
    let dirty = false;
    let throttle = 0;
    let poll = 0;
    let unwatch: (() => void) | null = null;
    const push = () => {
      if (throttle || !current) return;
      throttle = window.setTimeout(() => {
        throttle = 0;
        void read();
      }, PUSH);
    };
    const read = async () => {
      if (busy) {
        dirty = true;
        return;
      }
      busy = true;
      const running = await untrack(() => refreshOutput(item));
      busy = false;
      if (!current) return;
      alive = running;
      if (unwatch === null && running && item.port !== undefined) unwatch = watchServerLog(item.port, push);
      clearTimeout(poll);
      // A run that has not come up yet (or an unknown server) is still looked at; a dead one is not.
      if (running || item.status === 'running') poll = window.setTimeout(() => void read(), FALLBACK);
      if (dirty) {
        dirty = false;
        push();
      }
    };
    void read();
    return () => {
      current = false;
      clearTimeout(throttle);
      clearTimeout(poll);
      unwatch?.();
    };
  });

  function stop() {
    if (item.name === 'Run project' && item.port !== undefined) void stopServers(item.port);
    else if (item.name === 'Terminal' && item.terminalId) stopTerminalRun(item.terminalId);
  }
</script>

<!-- One box for the run: the command on top, a divider, then the output. The header stays put while the output scrolls. -->
{#if command || item.output}
  <div class="group/code relative rounded-md border border-border bg-background">
    {#if command}
      <pre class="m-0 overflow-x-auto py-1.5 pr-7 pl-2 font-mono whitespace-pre-wrap break-all">{command}</pre>
    {/if}
    {#if command && item.output}
      <div class="border-t border-border"></div>
    {/if}
    {#if item.output}
      <pre class="m-0 max-h-48 overflow-auto py-1 pr-7 pl-2 font-mono whitespace-pre-wrap break-all text-muted-foreground">{item.output}</pre>
    {/if}
    <button type="button" class="absolute top-1 right-1 grid size-4 cursor-pointer place-items-center rounded bg-background/80 text-muted-foreground backdrop-blur-sm outline-none hover:bg-primary/10 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring" aria-label="Copy" title="Copy" onclick={() => oncopy(copied)}>
      <ZIcon zType={'lucideCopy' as ZIconName} class="size-2.5" />
    </button>
  </div>
{/if}
{#if live && item.name !== 'Bash'}
  <button type="button" class="self-start cursor-pointer rounded-md border border-border px-2 py-0.5 text-[0.6875rem] text-muted-foreground outline-none hover:bg-destructive/10 hover:text-destructive" onclick={stop}>Stop</button>
{/if}
