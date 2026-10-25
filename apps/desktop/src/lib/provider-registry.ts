import type { ZIconName } from '@shival99/z-ui-svelte';
import { OPENCODE_SVG } from './brand-icons';

export type BuiltInAgent = 'claude' | 'codex' | 'antigravity' | 'opencode';
/** `icon` is an icon-set name; `mark`, when set, is the provider's own SVG and wins over `icon` everywhere. */
export type AgentProfile = { id: BuiltInAgent; label: string; binary: string; icon: ZIconName; mark?: string; tone: string };

/** Single UI registry for built-in agents. Session, workflow, composer, model and image surfaces read the same metadata. */
export const BUILT_IN_AGENTS: AgentProfile[] = [
  { id: 'claude', label: 'Claude', binary: 'claude', icon: 'brandClaude' as ZIconName, tone: 'text-orange-500' },
  { id: 'codex', label: 'Codex', binary: 'codex', icon: 'brandOpenai' as ZIconName, tone: 'text-foreground' },
  { id: 'antigravity', label: 'Antigravity', binary: 'agy', icon: 'brandGemini' as ZIconName, tone: 'text-sky-500' },
  { id: 'opencode', label: 'OpenCode', binary: 'opencode', icon: 'lucideTerminal' as ZIconName, mark: OPENCODE_SVG, tone: 'text-foreground' },
];

export const agentProfile = (id: string) => BUILT_IN_AGENTS.find((item) => item.id === id);
export const agentDisplayName = (id: string) => agentProfile(id)?.label ?? (id === 'custom' ? 'Custom provider' : id);
