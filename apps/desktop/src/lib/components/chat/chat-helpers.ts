import { AGENTS, chat } from '../../chat/sessions.svelte';
import { ticker } from '../../app/platform/clock.svelte';
import { answerText } from '../../chat/content/blocks';
import type { Segment } from '../../chat/content/blocks';

export const agentText = { claude: 'text-orange-500', codex: 'text-emerald-500', antigravity: 'text-sky-500', opencode: 'text-foreground', copilot: 'text-foreground', custom: 'text-violet-500' } as const;
export const agentName = (agent: string) => AGENTS.find((item) => item.value === agent)?.label ?? agent;

/** What the speaker button reads: the replies of the agent that run up to this one (no message of yours between). */
export function spokenRun(index: number) {
  const list = chat.messages;
  let from = index;
  while (from > 0 && list[from - 1].role === 'assistant') from--;
  return list
    .slice(from, index + 1)
    .map((reply) => answerText(reply.blocks))
    .filter(Boolean)
    .join('\n\n');
}

// Whether the part before this one, skipping blocks that draw nothing (empty reasoning), is also a step line.
export function afterSteps(parts: Segment[], position: number) {
  for (let index = position - 1; index >= 0; index--) {
    const part = parts[index];
    if (part.kind === 'commands') return true;
    if (part.kind === 'plan' || part.kind === 'tasks' || part.kind === 'agent_activity') return false;
    if (part.block.kind === 'thinking' && !part.block.text.trim()) continue;
    return false;
  }
  return false;
}

/** "1 minute ago", read again every minute. */
export function ago(at: number): string {
  const minutes = Math.floor((ticker.min - at) / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} hour${hours === 1 ? '' : 's'} ago` : `${Math.floor(hours / 24)} day${hours < 48 ? '' : 's'} ago`;
}
