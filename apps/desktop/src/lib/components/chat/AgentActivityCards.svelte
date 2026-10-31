<script lang="ts">
  import type { CommandBlock } from '../../chat/content/blocks';
  import { toolOf } from '../../chat/content/blocks';
  import type { ChatWork } from '../../chat/runs/chat-work';
  import { chatWork } from '../../chat/session/work.svelte';
  import { modelLabel, modelSources } from '../../agents/models.svelte';
  import BackgroundTasks from './BackgroundTasks.svelte';

  // The provider's own sub agents and background commands, listed exactly like the Work tray: one design for both.
  // `sessionId`: the chat the items run in; only rows with their own cancellation handle may show Stop.
  let { items, agent = 'codex', sessionId }: { items: CommandBlock[]; agent?: string; sessionId?: string } = $props();

  function identity(item: CommandBlock) {
    const label = item.task?.label ?? '';
    for (const source of modelSources()) {
      const model = source.models().find((entry) => label.startsWith(`${entry.value}:`));
      if (model) return { agent: source.provider, model: model.value, name: `${source.label} · ${model.label}`, task: label.slice(model.value.length + 1).trim() };
    }
    const [prefix, ...rest] = label.split(/:\s*/, 2);
    const looksLikeModelId = /^(?:gpt|claude|gemini|o\d|opencode)[\w./:-]*$/i.test(prefix ?? '');
    return {
      agent,
      model: undefined,
      // The label is the task text: the title names the tool, so the task is not shown twice.
      name: looksLikeModelId ? modelLabel(prefix) : toolOf(item.command) || 'Background task',
      task: looksLikeModelId ? rest.join(': ').trim() : label,
    };
  }

  // Each item becomes a Work tray row: the state follows the provider's task status.
  // The Work tray's row for each call, from the same projection: working, done, failed, stopped and paused read alike
  // in both places. A call the log no longer holds (an older chat, a reload) keeps the state its transcript recorded.
  const shared = $derived(new Map(chatWork(sessionId).flatMap((row) => (row.callId ? [[row.callId, row] as const] : []))));
  const work = $derived(items.map((item, index): ChatWork => {
    const task = item.task!;
    const info = identity(item);
    const recorded: ChatWork['state'] = task.status === 'started' || task.status === 'progress' ? 'running' : task.status === 'done' ? 'done' : task.status === 'failed' ? 'failed' : 'stopped';
    const live = item.id ? shared.get(item.id) : undefined;
    const state = live?.state ?? recorded;
    return {
      id: item.id ?? `activity-${index}`,
      sessionId: sessionId ?? '',
      title: info.task || task.summary || item.command,
      state,
      // A model-labelled task names its provider; a plain command shows as "Command".
      agent: info.model ? info.agent : undefined,
      model: info.model ?? null,
      kind: toolOf(item.command) === 'Bash' ? 'command' : 'agent',
      activityKey: live?.activityKey,
      canStop: state === 'running' && live?.canStop === true && Boolean(live.activityKey),
      startedAt: task.startedAt ?? Date.now(),
      endedAt: live?.endedAt ?? task.endedAt,
      detail: task.summary ?? item.output ?? '',
    };
  }));
</script>

<BackgroundTasks items={work} sessionId={sessionId ?? ''} title="Agent & background activity" stateKey={`activity:${items[0]?.id ?? 'none'}`} />
