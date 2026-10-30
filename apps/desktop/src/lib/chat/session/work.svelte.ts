import { getPref, setPref } from '../../app/platform/prefs';
import { activity } from '../runs/activity.svelte';
import { isWorkActive, projectChatWork, type ChatWork } from '../runs/chat-work';
import { delegateTasks } from './delegate-task.svelte';
import { delegateRuns } from './delegation.svelte';
import { manuallyStopped } from './events.svelte';

const KEY = 'chat.background-work.v1';
const restored = $state<ChatWork[]>([]);
let ready = $state(false);
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let pendingSnapshot: ChatWork[] = [];
void getPref<ChatWork[]>(KEY).then((saved) => {
  if (!Array.isArray(saved)) return;
  for (const item of saved) {
    if (!item || typeof item.id !== 'string' || typeof item.sessionId !== 'string' || typeof item.title !== 'string') continue;
    restored.push(isWorkActive(item) ? { ...item, state: 'unknown', canStop: false, endedAt: Date.now(), detail: `Last seen active before reload. Open the worker or inspect its output to confirm status.\n\n${item.detail}` } : item);
  }
}).catch(() => {}).finally(() => { ready = true; });

/** Work survives transcript eviction and chat switching; restored work never claims to be live without fresh events. */
export function chatWork(sessionId?: string): ChatWork[] {
  const current = projectChatWork(activity, Object.values(delegateTasks), delegateRuns).map((item) => {
    if (!item.delegateId && item.childId && manuallyStopped[item.childId]) return { ...item, state: 'paused' as const, canPause: false, canResume: true };
    return item;
  });
  const ids = new Set(current.map((item) => item.id));
  const all = [...current, ...restored.filter((item) => !ids.has(item.id))];
  return sessionId === undefined ? all : all.filter((item) => item.sessionId === sessionId);
}

$effect.root(() => {
  $effect(() => {
    if (!ready) return;
    const all = chatWork();
    const active = all.filter(isWorkActive);
    const recent = all.filter((item) => !isWorkActive(item)).sort((a, b) => b.startedAt - a.startedAt).slice(0, 100);
    pendingSnapshot = JSON.parse(JSON.stringify([...active, ...recent])) as ChatWork[];
    // Throttle instead of debounce: a continuously streaming worker still gets periodic recovery snapshots.
    saveTimer ??= setTimeout(() => {
      saveTimer = undefined;
      setPref(KEY, pendingSnapshot);
    }, 300);
  });
});
