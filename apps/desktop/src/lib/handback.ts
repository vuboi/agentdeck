// The prompt that hands the answers of other agents (`@agent`, a flow's steps) back to the chat's own agent. It is sent
// to the agent but never shown: the chat reads as one turn, the agent's reply simply continues after the delegate card.

/** How the hand-back prompt starts. */
export const HANDBACK_HEAD = 'The agents you handed work to have answered.';

const AGENTDECK_HINT_START = '[AgentDeck] ';
const EARLIER_START = 'Earlier in this chat, other agents answered';
const NOW_MARK = 'Now the user says:\n';

/** A prompt read back from a chat's file as the user wrote it: without the AgentDeck lead paragraph put before it. */
export function stripLead(text: string): string {
  if (!text.startsWith(AGENTDECK_HINT_START)) return text;
  const cut = text.indexOf('\n\n');
  return cut < 0 ? '' : text.slice(cut + 2).trimStart();
}

/**
 * Whether a prompt read back from a chat's file is a hand-back. Before the head the prompt can carry the AgentDeck hint
 * (a paragraph) and the digest of the turns this agent missed (ends with "Now the user says:").
 */
export function isHandbackPrompt(text: string): boolean {
  let rest = text.trimStart();
  if (rest.startsWith(AGENTDECK_HINT_START)) {
    const cut = rest.indexOf('\n\n');
    rest = cut < 0 ? '' : rest.slice(cut + 2).trimStart();
  }
  if (rest.startsWith(EARLIER_START)) {
    const at = rest.indexOf(NOW_MARK);
    rest = at < 0 ? '' : rest.slice(at + NOW_MARK.length).trimStart();
  }
  return rest.startsWith(HANDBACK_HEAD);
}
