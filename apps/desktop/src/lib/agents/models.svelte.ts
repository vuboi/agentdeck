import type { ZIconName } from '@shival99/z-ui-svelte';
import { inTauri } from '../app/platform/history';
import { claudeCreditOn, connectors } from './connectors.svelte';
import { CREDIT_PREFIX, creditModel } from './credit-model';
import { agentProfile } from './provider-registry';
import { providerLogo } from './brand-icons';

// Model catalogue for the picker, per provider. It starts from a small built-in list and is replaced at start-up
// by what each CLI keeps on disk (Rust `models_list`): Codex's cached catalogue with each model's reasoning
// levels, and Claude Code's current tiers plus the extra models the account is offered.

export type ProviderId = 'claude' | 'codex' | 'antigravity' | 'opencode' | 'copilot' | 'custom';
export type ModelOption = {
  value: string;
  label: string;
  description?: string;
  /** Reasoning levels the model takes; empty when unknown. */
  efforts?: string[];
  defaultEffort?: string;
  /** The CLI keeps it out of its own picker; it still runs. Listed last, under "Hide". */
  hidden?: boolean;
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
const THINKING = ['off', 'low', 'medium', 'high'];

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
  // Free models read from this CLI at start-up (`opencode models`); the same ids as a fallback when it is missing.
  // OpenCode's reasoning levels are model variants the CLI does not list, so no effort is offered here.
  opencode: [
    { value: 'opencode/big-pickle', label: 'Big Pickle' },
    { value: 'opencode/beam-free', label: 'Beam (Free)' },
    { value: 'opencode/glm-5.3-flash-free', label: 'GLM 5.3 Flash (Free)' },
    { value: 'opencode/deepseek-v4.1-flash-free', label: 'DeepSeek V4.1 Flash (Free)' },
    { value: 'opencode/flint-40b-free', label: 'Flint 40B (Free)' },
    { value: 'opencode/mimo-v2.6-flash-free', label: 'MiMo V2.6 Flash (Free)' },
    { value: 'opencode/nemotron-3-ultra-free', label: 'Nemotron 3 Ultra (Free)' },
    { value: 'opencode/nemotron-3.5-lightning-free', label: 'Nemotron 3.5 Lightning (Free)' },
    { value: 'opencode/nemotron-3.5-super-vl-free', label: 'Nemotron 3.5 Super VL (Free)' },
  ],
  copilot: [],
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
const chattable = () => connectors.custom.filter((provider) => provider.enabled && provider.kind !== 'anthropic');

// Claude's own models, as the CLI lists them. The catalogue's Claude list is these, then, while a Console API key is
// saved, the same models billed to the API credit (`credit:` ids, after a "Claude credit" divider in the pickers).
let claudeOwn = $state<ModelOption[]>([...catalog.claude]);

$effect.root(() => {
  $effect(() => {
    const credit = claudeCreditOn()
      ? claudeOwn.map((item) => ({ ...item, value: `${CREDIT_PREFIX}${item.value}`, description: 'Claude credit · billed to the API credit' }))
      : [];
    catalog.claude = [...claudeOwn, ...credit];
  });
  $effect(() => {
    catalog.custom = chattable().flatMap((provider) => {
      const names = provider.models.length ? provider.models : provider.defaultModel ? [provider.defaultModel] : [];
      return names.map((name) => ({ value: customModelId(provider.id, name), label: name, description: provider.name, efforts: THINKING, defaultEffort: 'off' }));
    });
  });
});

// The provider icons come from the registry, the same as every other surface.
const iconOf = (id: string) => (agentProfile(id)?.icon ?? 'lucideServer') as ZIconName;
const builtIn: ModelSource[] = [
  { key: 'claude', provider: 'claude', label: 'Claude', icon: iconOf('claude'), models: () => catalog.claude },
  { key: 'codex', provider: 'codex', label: 'Codex', icon: iconOf('codex'), models: () => catalog.codex },
  { key: 'antigravity', provider: 'antigravity', label: 'Antigravity', icon: iconOf('antigravity'), models: () => catalog.antigravity },
  { key: 'opencode', provider: 'opencode', label: 'OpenCode', icon: iconOf('opencode'), models: () => catalog.opencode },
  { key: 'copilot', provider: 'copilot', label: 'GitHub Copilot', icon: iconOf('copilot'), models: () => catalog.copilot },
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
      logo: providerLogo(provider),
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

type Listed = { value: string; label: string; description: string; efforts: string[]; default_effort: string | null; hidden?: boolean };

/** Reads each CLI's own model list; a CLI that has none (not installed, never run) keeps the built-in one. */
/**
 * Whether each CLI's own model list has been read yet. Until it has, the list in the catalogue is only a fallback: a
 * picker shows a loading state instead of it, so the list is never drawn and then replaced and reordered.
 */
export const modelsReady = $state<Record<ProviderId, boolean>>({ claude: false, codex: false, antigravity: false, opencode: false, copilot: false, custom: true });

export async function loadModels(providers: readonly ProviderId[] = ['claude', 'codex', 'antigravity', 'opencode', 'copilot']) {
  if (!inTauri()) {
    for (const provider of Object.keys(modelsReady) as ProviderId[]) modelsReady[provider] = true;
    return;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  // All at once: a slow CLI (OpenCode starts a process to list its models) must not hold the others back.
  await Promise.all(
    providers.map(async (provider) => {
      try {
        const listed = await invoke<Listed[]>('models_list', { agent: provider });
        if (listed.length) {
          const models = listed.map((item) => ({
            value: item.value,
            label: item.label,
            description: item.description || undefined,
            efforts: item.efforts,
            defaultEffort: item.default_effort ?? undefined,
            hidden: item.hidden || undefined,
          }));
          if (provider === 'claude') claudeOwn = models;
          else catalog[provider] = models;
        }
      } catch {
        // Keep the built-in list.
      } finally {
        modelsReady[provider] = true;
      }
    }),
  );
}

/** Re-reads model catalogues whose account-backed CLI changed, showing loading instead of the previous account's list. */
export async function reloadModels(providers: readonly ProviderId[]) {
  for (const provider of providers) {
    modelsReady[provider] = false;
    // Do not expose the previous account's catalogue while the new credential is being read.
    if (provider === 'claude') claudeOwn = [];
    else catalog[provider] = [];
  }
  await loadModels(providers);
}

void loadModels();

/**
 * A short name for a model id, for message headers: the catalogue's label when the model is listed, else the id
 * made readable (`claude-opus-5-5` becomes `Opus 5.5`, `gpt-5.6-luna` becomes `GPT 5.6 Luna`).
 */
export function modelLabel(value: string | undefined | null): string {
  if (!value) return '';
  const custom = parseCustomModel(value);
  if (custom) return modelLabel(custom.model) || custom.model;
  // The same name the pickers show: `credit:` models carry the "· Credit" suffix.
  const credit = creditModel(value);
  if (credit) return `${modelLabel(credit)} · Credit`;
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

/** A free model: its id ends in `-free` (OpenCode) or `:free` (OpenRouter). */
export const isFreeModel = (item: { value: string }) => /[-:]free$/i.test(item.value);

/**
 * A model that needs credit on top of the plan (Claude's Fable): it works only once money is added, so the pickers
 * list it last, under the "Paid" divider. Credit variants (`credit:`) have their own divider.
 */
export const isPaidOnlyModel = (item: { value: string }) => !item.value.startsWith(CREDIT_PREFIX) && /fable/i.test(item.value);

/** A divider of the model lists: where it goes (the index of the model it stands above) and its name. */
export type ModelDivider = { at: number; label: string };

/**
 * A model list in the order the pickers show it, with the dividers between its groups: Free first, then Hide (the ones
 * the CLI keeps out of its own picker), then Paid; without free models, the ones the plan covers, Hide, then those that
 * need credit; the credit variants last (Claude credit). A divider is drawn only where it separates something: "Free" above the free
 * models when others follow, "Paid" where the paid ones start, and so on.
 */
export function freeFirst<T extends { value: string; hidden?: boolean }>(items: T[]): { items: T[]; dividers: ModelDivider[] } {
  const credit = items.filter((item) => item.value.startsWith(CREDIT_PREFIX));
  const own = items.filter((item) => !item.value.startsWith(CREDIT_PREFIX));
  const hidden = own.filter((item) => item.hidden);
  const shown = own.filter((item) => !item.hidden);
  const free = shown.filter(isFreeModel);
  const rest = shown.filter((item) => !isFreeModel(item));
  const included = rest.filter((item) => !isPaidOnlyModel(item));
  const paidOnly = rest.filter(isPaidOnlyModel);
  // Free, then Hide, then Paid. Where nothing is free (Codex, Claude) the models the plan covers come first, the hidden
  // ones after them, then those that need credit.
  const groups: [string, T[]][] = free.length
    ? [['Free', free], ['Hide', hidden], ['Paid', included], ['Paid', paidOnly], ['Claude credit', credit]]
    : [['', included], ['Hide', hidden], ['Paid', paidOnly], ['Claude credit', credit]];
  const dividers: ModelDivider[] = [];
  const named = new Set<string>();
  let at = 0;
  for (const [label, group] of groups) {
    if (group.length && label && !named.has(label) && (at > 0 || label === 'Free')) {
      dividers.push({ at, label });
      named.add(label);
    }
    at += group.length;
  }
  return { items: groups.flatMap(([, group]) => group), dividers };
}
