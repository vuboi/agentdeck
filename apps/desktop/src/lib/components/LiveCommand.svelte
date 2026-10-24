<script lang="ts">
  import { refreshOutput, type Activity } from '../activity.svelte';
  import { stopServers } from '../browser.svelte';
  import { stopTerminalRun } from '../terminal.svelte';
  import CommandOutput from './CommandOutput.svelte';

  // The card of one run / terminal / shell command in the Tasks panel. While it is open and the thing still runs, its
  // output is read again every 1.5 s; a finished one keeps the last text.
  let { item, oncopy }: { item: Activity; oncopy: (text: string) => void } = $props();

  const polled = $derived(item.name === 'Run project' || item.name === 'Terminal');
  let alive = $state(false);
  const live = $derived(polled ? alive : item.status === 'running' || item.status === 'background');
  // The command line only: a run's input also names its folder on a second line.
  const command = $derived(item.name === 'Run project' ? item.input.split('\n')[0] : item.input);

  $effect(() => {
    if (!polled) return;
    let current = true;
    let timer = 0;
    const tick = async () => {
      const running = await refreshOutput(item);
      if (!current) return;
      alive = running;
      // A run that has not come up yet (or an unknown server) is still looked at; a dead one is not.
      if (running || item.status === 'running') timer = window.setTimeout(tick, 1500);
    };
    void tick();
    return () => {
      current = false;
      clearTimeout(timer);
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
