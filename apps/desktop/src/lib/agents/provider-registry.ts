import type { ZIconName } from '@shival99/z-ui-svelte';
import { CLAUDE_LOGO, CLAUDE_SVG, COPILOT_LOGO, GEMINI_LOGO, GEMINI_SVG, GITHUB_SVG, OPENAI_SVG, OPENCODE_SVG, connectorLogo, providerLogo } from './brand-icons';
import { connectors } from './connectors.svelte';

export type BuiltInAgent = 'claude' | 'codex' | 'antigravity' | 'opencode' | 'copilot';
/** `icon` is an icon-set name; `mark`, when set, is the provider's own SVG and wins over `icon` everywhere. */
export type AgentProfile = { id: BuiltInAgent; label: string; binary: string; icon: ZIconName; mark?: string; image?: string; tone: string };

/** Single UI registry for built-in agents. Session, workflow, composer, model and image surfaces read the same metadata. */
export const BUILT_IN_AGENTS: AgentProfile[] = [
  { id: 'claude', label: 'Claude', binary: 'claude', icon: 'brandClaude' as ZIconName, mark: CLAUDE_SVG, image: CLAUDE_LOGO, tone: 'text-orange-500' },
  { id: 'codex', label: 'Codex', binary: 'codex', icon: 'brandOpenai' as ZIconName, mark: OPENAI_SVG, tone: 'text-foreground' },
  { id: 'antigravity', label: 'Antigravity', binary: 'agy', icon: 'brandGemini' as ZIconName, mark: GEMINI_SVG, image: GEMINI_LOGO, tone: 'text-sky-500' },
  { id: 'opencode', label: 'OpenCode', binary: 'opencode', icon: 'lucideTerminal' as ZIconName, mark: OPENCODE_SVG, tone: 'text-foreground' },
  { id: 'copilot', label: 'GitHub Copilot', binary: 'gh', icon: 'lucideBot' as ZIconName, mark: GITHUB_SVG, image: COPILOT_LOGO, tone: 'text-violet-500' },
];

export const agentProfile = (id: string) => BUILT_IN_AGENTS.find((item) => item.id === id);

// ---------- custom providers ----------

/** The custom provider a chat, step or reply used: named by its model (`<provider id>::<model>`, or the bare model). */
export function customProviderOf(model?: string | null) {
  const at = model?.indexOf('::') ?? -1;
  const id = model && at > 0 ? model.slice(0, at) : '';
  const enabled = connectors.custom.filter((item) => item.enabled);
  if (id) return connectors.custom.find((item) => item.id === id);
  // A reply keeps the bare model name (`deepseek/deepseek-v4.1-flash`): the provider is the one that lists it, or the
  // only custom provider there is.
  return (model ? enabled.find((item) => item.models.includes(model) || item.defaultModel === model) : undefined) ?? (enabled.length === 1 ? enabled[0] : undefined);
}

/** The name the user gave the custom provider of `model`; `Custom provider` when it cannot be told. */
export const customProviderLabel = (model?: string | null) => customProviderOf(model)?.name ?? 'Custom provider';

export const agentDisplayName = (id: string, model?: string | null) =>
  agentProfile(id)?.label ?? (id === 'custom' ? customProviderLabel(model) : id);

// ---------- what to draw for an agent ----------

/** How to draw an agent: an image (a custom provider's logo, uploaded or its brand's), an SVG mark, or an icon-set glyph. */
export type AgentLook =
  | { kind: 'image'; src: string; tone: string }
  | { kind: 'mark'; svg: string; tone: string }
  | { kind: 'icon'; name: ZIconName; tone: string };

/**
 * The one answer to "what does this agent look like": every logo of an agent in the app (chat headers, flow cards,
 * workflow nodes, task cards, pickers, stacks) is drawn from this, so a change here reaches all of them. A custom
 * provider (named by `model`) shows its own logo, else its brand's, else a server glyph.
 */
export function agentLook(agent: string, model?: string | null): AgentLook {
  if (agent === 'custom') {
    const provider = customProviderOf(model);
    const src = provider ? providerLogo(provider) : undefined;
    return src ? { kind: 'image', src, tone: '' } : { kind: 'icon', name: 'lucideServer' as ZIconName, tone: 'text-violet-500' };
  }
  const profile = agentProfile(agent);
  if (profile?.image) return { kind: 'image', src: profile.image, tone: profile.tone };
  if (!profile) {
    const logo = connectorLogo(agent);
    if (logo) return { kind: 'image', src: logo, tone: '' };
    return { kind: 'icon', name: 'lucideBot' as ZIconName, tone: 'text-muted-foreground' };
  }
  return profile.mark ? { kind: 'mark', svg: profile.mark, tone: profile.tone } : { kind: 'icon', name: profile.icon, tone: profile.tone };
}
