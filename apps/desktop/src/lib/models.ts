import type { ZIconName } from '@shival99/z-ui-svelte';

// Model catalogue for the picker. One source per provider; the picker only reads `modelSources`,
// so a live source (the CLI's own model cache, read through a Tauri command) can replace a static
// one later without touching any component.

export type ProviderId = 'claude' | 'codex' | 'gemini';
export type ModelOption = { value: string; label: string };

export type ModelSource = {
  provider: ProviderId;
  label: string;
  icon: ZIconName;
  /** Models this provider exposes. Static for now. */
  models: () => ModelOption[];
};

// Claude Code: IDs as they appear in ~/.claude.json (claude-<tier>-5-5).
// Codex: slugs from ~/.codex/models_cache.json, the entries with visibility "list".
// Gemini / Antigravity: no local list found yet, so these are placeholders.
export const modelSources: ModelSource[] = [
  {
    provider: 'claude',
    label: 'Claude',
    icon: 'brandClaude' as ZIconName,
    models: () => [
      { value: 'claude-opus-5-5', label: 'Opus 5.5' },
      { value: 'claude-sonnet-5-5', label: 'Sonnet 5.5' },
      { value: 'claude-haiku-5-5', label: 'Haiku 5.5' },
    ],
  },
  {
    provider: 'codex',
    label: 'Codex',
    icon: 'brandOpenai' as ZIconName,
    models: () => [
      { value: 'gpt-6-luna', label: 'GPT-6 Luna' },
      { value: 'gpt-5.6-terra', label: 'GPT-5.6 Terra' },
      { value: 'gpt-5.6-luna', label: 'GPT-5.6 Luna' },
    ],
  },
  {
    provider: 'gemini',
    label: 'Gemini',
    icon: 'brandGemini' as ZIconName,
    models: () => [
      { value: 'gemini-3-pro', label: 'Gemini 3 Pro' },
      { value: 'gemini-3-flash', label: 'Gemini 3 Flash' },
    ],
  },
];

/** Provider id -> its models. Components read this instead of a hard-coded table. */
export function modelCatalog(): Record<ProviderId, ModelOption[]> {
  return Object.fromEntries(modelSources.map((source) => [source.provider, source.models()])) as Record<ProviderId, ModelOption[]>;
}
