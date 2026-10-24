import type { ZIconName } from '@shival99/z-ui-svelte';
import { inTauri } from './history';
import { connectors } from './connectors.svelte';

// Model catalogue for the picker, per provider. It starts from a small built-in list and is replaced at start-up
// by what each CLI keeps on disk (Rust `models_list`): Codex's cached catalogue with each model's reasoning
// levels, and Claude Code's current tiers plus the extra models the account is offered.

export type ProviderId = 'claude' | 'codex' | 'antigravity' | 'custom';
export type ModelOption = {
  value: string;
  label: string;
  description?: string;
  /** Reasoning levels the model takes; empty when unknown. */
  efforts?: string[];
  defaultEffort?: string;
};

export type ModelSource = {
  /** Unique row key: the provider id, or one per custom provider. */
  key: string;
  provider: ProviderId;
  label: string;
  icon: ZIconName;
  /** An uploaded logo (a `data:` URL) that replaces the icon. */
  logo?: string;
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
  // Antigravity has no built-in list: it is read from `agy models` at start-up.
  antigravity: [],
  // Every model of every enabled custom provider: filled from the saved providers (see below).
  custom: [],
});

/** Id of a custom provider's model in the catalogue and in a chat: `<provider id>::<model>`. */
export const customModelId = (providerId: string, model: string) => `${providerId}::${model}`;
/** The custom provider and model a catalogue id points at. */
export function parseCustomModel(value: string | null | undefined) {
  const at = value?.indexOf('::') ?? -1;
  return value && at > 0 ? { providerId: value.slice(0, at), model: value.slice(at + 2) } : null;
}

/** What a local model's thinking can be set to: off, or a level (sent as `reasoning_effort`). */
const THINKING = ['off', 'low', 'medium', 'high'];

const chattable = () => connectors.custom.filter((provider) => provider.enabled && provider.kind !== 'anthropic');

$effect.root(() => {
  $effect(() => {
    catalog.custom = chattable().flatMap((provider) => {
      const names = provider.models.length ? provider.models : provider.defaultModel ? [provider.defaultModel] : [];
      return names.map((name) => ({ value: customModelId(provider.id, name), label: name, description: provider.name, efforts: THINKING, defaultEffort: 'off' }));
    });
  });
});

const builtIn: ModelSource[] = [
  { key: 'claude', provider: 'claude', label: 'Claude', icon: 'brandClaude' as ZIconName, models: () => catalog.claude },
  { key: 'codex', provider: 'codex', label: 'Codex', icon: 'brandOpenai' as ZIconName, models: () => catalog.codex },
  { key: 'antigravity', provider: 'antigravity', label: 'Antigravity', icon: 'brandGemini' as ZIconName, models: () => catalog.antigravity },
];

/** The rows of the model picker: the three CLIs, then one per enabled custom provider. Reactive. */
export function modelSources(): ModelSource[] {
  return [
    ...builtIn,
    ...chattable().map((provider) => ({
      key: provider.id,
      provider: 'custom' as const,
      label: provider.name,
      icon: 'lucideServer' as ZIconName,
      logo: provider.logo,
      models: () => catalog.custom.filter((item) => parseCustomModel(item.value)?.providerId === provider.id),
    })),
  ];
}

/** Provider id -> its models (the live object: it changes when the CLIs' lists are read). */
export function modelCatalog(): Record<ProviderId, ModelOption[]> {
  return catalog;
}

/** The name the user gave a custom provider, for a custom model id; `undefined` for any other model. */
export function customProviderName(value: string | undefined | null): string | undefined {
  const target = parseCustomModel(value);
  return target ? (connectors.custom.find((item) => item.id === target.providerId)?.name ?? 'Custom provider') : undefined;
}

/** The model entry for a provider and id, if listed. */
export const findModel = (provider: ProviderId, value: string) => catalog[provider]?.find((item) => item.value === value);

type Listed = { value: string; label: string; description: string; efforts: string[]; default_effort: string | null };

/** Reads each CLI's own model list; a CLI that has none (not installed, never run) keeps the built-in one. */
export async function loadModels() {
  if (!inTauri()) return;
  const { invoke } = await import('@tauri-apps/api/core');
  for (const provider of ['claude', 'codex', 'antigravity'] as const) {
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

/**
 * A short name for a model id, for message headers: the catalogue's label when the model is listed, else the id
 * made readable (`claude-opus-5-5` becomes `Opus 5.5`, `gpt-5.6-luna` becomes `GPT 5.6 Luna`).
 */
export function modelLabel(value: string | undefined | null): string {
  if (!value) return '';
  const listed = Object.values(catalog)
    .flat()
    .find((item) => item.value === value);
  if (listed) return listed.label;
  const isNumber = (text: string) => /^\d+(\.\d+)*$/.test(text);
  const parts = value
    .replace(/\[.*\]$/, '')
    .replace(/-\d{8}$/, '')
    .split('-')
    .filter((part, index) => !(index === 0 && part === 'claude'));
  const words: string[] = [];
  for (const part of parts) {
    const last = words.at(-1);
    if (isNumber(part) && last !== undefined && isNumber(last)) words[words.length - 1] = `${last}.${part}`;
    else words.push(part === 'gpt' ? 'GPT' : isNumber(part) ? part : part.charAt(0).toUpperCase() + part.slice(1));
  }
  return words.join(' ');
}
