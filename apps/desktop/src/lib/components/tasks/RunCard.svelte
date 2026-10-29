<script lang="ts">
  import type { Activity } from '../../chat/runs/activity.svelte';
  import { toast } from '../../app/platform/toast';
  import LiveCommand from '../chat/LiveCommand.svelte';

  // The live output of one command listed under "Running in the background": a terminal tab's command or a dev
  // server. It is the same card the Tasks panel shows, fed the same way (the terminal pushes, the Rust side announces a
  // server's log), and it exists only while the row is open, so a closed row costs nothing.
  type Props = { terminalId?: string; port?: number; command: string; cwd?: string; startedAt: number; running?: boolean };
  const { terminalId, port, command, cwd, startedAt, running = true }: Props = $props();

  // An entry of its own for the card (not one of the audit log's): its output fills in while the card is open.
  // svelte-ignore state_referenced_locally
  const item = $state<Activity>({
    key: `background:${terminalId ?? port}`,
    sessionId: '',
    title: '',
    name: terminalId ? 'Terminal' : 'Run project',
    input: terminalId ? command : `${command}\n${cwd ?? ''}`,
    background: false,
    status: running ? 'running' : 'stopped',
    startedAt,
    terminalId,
    port,
  });
  $effect(() => {
    item.status = running ? 'running' : 'stopped';
  });
</script>

<div class="w-full basis-full">
  <LiveCommand {item} oncopy={(text) => void navigator.clipboard.writeText(text).then(() => toast.success('Output copied'))} />
</div>
