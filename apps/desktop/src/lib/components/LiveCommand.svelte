<script lang="ts">
  import { untrack } from 'svelte';
  import { refreshOutput, watchServerLog, watchTerminalOutput, type Activity } from '../activity.svelte';
  import { stopServers } from '../browser.svelte';
  import { stopTerminalRun } from '../terminal.svelte';
  import CommandOutput from './CommandOutput.svelte';

  // The card of one run / terminal / shell command in the Tasks panel (it exists only while the entry is open). A
  // Terminal entry is fed by its tab: the terminal pushes the new output as the shell prints it. A Run project entry
  // reads the server's log again when the Rust side says it grew (`server-log`), and once a second as a fallback, while
  // the server runs. A finished one keeps the last text and nothing runs for it.
  let { item, oncopy }: { item: Activity; oncopy: (text: string) => void } = $props();

  let alive = $state(false);
  const live = $derived(item.name === 'Run project' ? alive : item.status === 'running' || item.status === 'background');
  // The command line only: a run's input also names its folder on a second line.
  const command = $derived(item.name === 'Run project' ? item.input.split('\n')[0] : item.input);

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

<CommandOutput
  title={item.name === 'Run project' ? 'Run project' : item.name}
  {command}
  output={item.output ?? ''}
  {live}
  startedAt={item.startedAt}
  endedAt={item.endedAt}
  onstop={item.name === 'Bash' ? undefined : stop}
  {oncopy}
/>
