import type { ZIconName } from '@shival99/z-ui-svelte';
import { inTauri } from './history';

// Model catalogue for the picker, per provider. It starts from a small built-in list and is replaced at start-up
// by what each CLI keeps on disk (Rust `models_list`): Codex's cached catalogue with each model's reasoning
// levels, and Claude Code's current tiers plus the extra models the account is offered.

export type ProviderId = 'claude' | 'codex' | 'gemini';
export type ModelOption = {
  value: string;
  label: string;
  description?: string;
  /** Reasoning levels the model takes; empty when unknown. */
  efforts?: string[];
  defaultEffort?: string;
};

export type ModelSource = {
  provider: ProviderId;
  label: string;
  icon: ZIconName;
  models: () => ModelOption[];
};

const CLAUDE_EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'];

/** The lists the picker reads. Reactive: a component showing them updates when the live lists arrive. */
const catalog = $state<Record<ProviderId, ModelOption[]>>({
  claude: [
    { value: 'claude-opus-5-5', label: 'Opus 5.5', efforts: CLAUDE_EFFORTS, defaultEffort: 'medium' },
    { value: 'claude-sonnet-5-5', label: 'Sonnet 5.5', efforts: CLAUDE_EFFORTS, defaultEffort: 'high' },
    { value: 'claude-haiku-5-5', label: 'Haiku 5.5', efforts: CLAUDE_EFFORTS, defaultEffort: 'medium' },
  ],
  codex: [
    { value: 'gpt-6-luna', label: 'GPT 6 Luna' },
    { value: 'gpt-5.6-terra', label: 'GPT 5.6 Terra' },
  ],
  // Gemini / Antigravity: no local list found yet, so these are placeholders.
  gemini: [
    { value: 'gemini-3-pro', label: 'Gemini 3 Pro' },
    { value: 'gemini-3-flash', label: 'Gemini 3 Flash' },
  ],
});

export const modelSources: ModelSource[] = [
  { provider: 'claude', label: 'Claude', icon: 'brandClaude' as ZIconName, models: () => catalog.claude },
  { provider: 'codex', label: 'Codex', icon: 'brandOpenai' as ZIconName, models: () => catalog.codex },
  { provider: 'gemini', label: 'Gemini', icon: 'brandGemini' as ZIconName, models: () => catalog.gemini },
];

/** Provider id -> its models (the live object: it changes when the CLIs' lists are read). */
export function modelCatalog(): Record<ProviderId, ModelOption[]> {
  return catalog;
}

/** The model entry for a provider and id, if listed. */
export const findModel = (provider: ProviderId, value: string) => catalog[provider]?.find((item) => item.value === value);

type Listed = { value: string; label: string; description: string; efforts: string[]; default_effort: string | null };

/** Reads each CLI's own model list; a CLI that has none (not installed, never run) keeps the built-in one. */
export async function loadModels() {
  if (!inTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  for (const provider of ['claude', 'codex'] as const) {
    try {
      const listed = await invoke<Listed[]>('models_list', { agent: provider });
      if (listed.length) {
        catalog[provider] = listed.map((item) => ({
          value: item.value,
          label: item.label,
          description: item.description || undefined,
          efforts: item.efforts,
          defaultEffort: item.default_effort ?? undefined,
        }));
      }
    } catch {
      // Keep the built-in list.
    }
  }
}
void loadModels();
