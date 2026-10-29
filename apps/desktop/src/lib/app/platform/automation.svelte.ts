// What the app does on its own around a chat, switched in the model popover of the message box:
// - compress: before a Claude turn whose context is near the model's window, run `/compact` first (Codex compacts by
//   itself), with a "Compressing context" line in the chat;
// - fallback: at a usage limit, a sign-out or a server error, the next agent of the chain carries on
//   (`connectors.recovery.enabled`, turn-failure.ts);
// - orchestrate: the agent is told it may hand light or separable parts to other agents (delegate_task).
import { getPref, setPref } from './prefs';
import { connectors } from '../../agents/connectors.svelte';

export const automation = $state({ compress: false, orchestrate: true });

const KEY = 'chat.automation';
let hydrated = false;
void getPref<Partial<typeof automation>>(KEY).then((saved) => {
  if (saved) Object.assign(automation, saved);
  hydrated = true;
});

$effect.root(() => {
  $effect(() => {
    const value = { ...automation };
    if (hydrated) setPref(KEY, value);
  });
});

/** Moving to another agent when one fails: off until it is switched on (it used to run by itself). */
export const fallbackOn = () => connectors.recovery.enabled;
export function setFallback(on: boolean) {
  connectors.recovery = { ...connectors.recovery, enabled: on, chosen: true };
}

/** Share of the model's window a Claude context may fill before the next turn compacts it first. */
export const COMPRESS_AT = 0.75;

/** The context window of a Claude model, in tokens: `[1m]` models have a million, the others 200k. */
export const contextWindow = (model: string | undefined | null) => (model && /\[1m\]/i.test(model) ? 1_000_000 : 200_000);
