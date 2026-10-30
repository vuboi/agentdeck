// What the app does on its own around a chat, switched in the model popover of the message box:
// - compress: before a Claude turn whose context is near the model's window, run `/compact` first (Codex compacts by
//   itself), with a "Compressing context" line in the chat;
// - fallback: at a usage limit, a sign-out or a server error, the next agent of the chain carries on
//   (`connectors.recovery.enabled`, turn-failure.ts);
// - orchestrate: the agent is told it may hand light or separable parts to other agents (delegate_task).
import { getPref, setPref } from './prefs';
import { connectors } from '../../agents/connectors.svelte';

export const automation = $state({ compress: true, orchestrate: true, prehandle: true });

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

/** A chat's context is compacted once it reaches this many tokens, for every model. */
export const COMPRESS_TOKENS = 500_000;
