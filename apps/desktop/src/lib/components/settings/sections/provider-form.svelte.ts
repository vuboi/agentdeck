import { ENDPOINTS } from '../../../agents/endpoints';
import { connectors, newProviderId, probeProvider, refreshClaudeCredit, saveProvider, type CustomProvider } from '../../../agents/connectors.svelte';

export const KINDS = [
  { value: 'openai', label: 'OpenAI-compatible', hint: 'LM Studio, llama.cpp, vLLM, OpenRouter, LiteLLM…', base: ENDPOINTS.providerKinds.openai },
  { value: 'anthropic', label: 'Anthropic-compatible', hint: 'Anthropic API or a proxy that speaks it', base: ENDPOINTS.providerKinds.anthropic },
  { value: 'ollama', label: 'Ollama', hint: 'Local models served by Ollama', base: ENDPOINTS.providerKinds.ollama },
] as const;

/** Official APIs with a free tier, all OpenAI-compatible. The key is read from the named environment variable when none is typed. */
export const FREE_CLOUD = ENDPOINTS.freeCloud;

/** A preset's address against a saved one. A preset with `{account_id}` (Cloudflare) matches any account. */
export function samePreset(base: string, url: string) {
  const clean = url.trim().replace(/\/+$/, '');
  const [head, tail] = base.split('{account_id}');
  return tail === undefined ? clean === base : clean.startsWith(head) && clean.endsWith(tail) && clean.length > head.length + tail.length;
}

/** The account of a preset whose address holds one (Cloudflare): typed in the form, put into the address. */
const accountOf = (base: string, url: string) => {
  const [head, tail] = base.split('{account_id}');
  return tail !== undefined && url.startsWith(head) && url.endsWith(tail) ? url.slice(head.length, url.length - tail.length).replace('{account_id}', '') : '';
};

/** A Claude Console key: the Anthropic API with a key, shown with the Claude logo and read for the credit left. */
export const isClaudeConsole = (provider: { kind: string; baseUrl: string }) =>
  provider.kind === 'anthropic' && provider.baseUrl.trim().replace(/\/+$/, '') === ENDPOINTS.anthropic.api;

const blank = (): CustomProvider => {
  const keyId = `key-${Date.now().toString(36)}`;
  return { id: newProviderId(), name: '', kind: 'openai', baseUrl: KINDS[0].base, keyEnv: '', keys: [{ id: keyId, label: 'Key 1', apiKey: '' }], activeKeyId: keyId, autoSwitch: false, models: [], defaultModel: '', enabled: true };
};

/**
 * State and actions of the provider form, shared by the providers list (which opens it) and the form modals
 * (which stay in the settings shell so they sit above the panel).
 */
export class ProviderForm {
  draft = $state<CustomProvider | null>(null);
  probing = $state(false);
  probeNote = $state<{ ok: boolean; text: string } | null>(null);
  /** The key the provider being edited was saved with: shown hidden, never in the field (SecretInput). */
  keptKey = $state('');
  keptKeys = $state<Record<string, string>>({});
  /** The provider form is a modal over the settings (like adding an MCP server). */
  providerModal = $state(false);
  account = $state('');
  /** The free cloud provider being edited, found by its address: its name and logo are fixed. */
  // A getter, not $derived: this class is made inside the settings modal, and a $derived would belong to that component's
  // effect scope, so reading it after the modal closes warns (derived_inert). A getter still reads the same state.
  get freePreset() {
    return this.draft ? FREE_CLOUD.find((preset) => samePreset(preset.base, this.draft?.baseUrl ?? '') || preset.base === this.draft?.baseUrl) : undefined;
  }

  /** The draft is a Claude Console key: the Claude logo and name are fixed, and the Console sign-in is asked for. */
  get isClaude() {
    return this.draft ? isClaudeConsole(this.draft) : false;
  }

  /** The name a provider cannot change: a free cloud provider's, or the Claude Console's. */
  get fixedName() {
    return this.freePreset?.name ?? (this.isClaude ? 'Claude Console' : undefined);
  }

  /** The Console sign-in (`sessionKey`) read for the credit left; one for the account, so it is shared by every Claude key. */
  consoleSession = $state('');

  edit(provider?: CustomProvider) {
    // A provider saved before keys were kept as text has none: the field binds a string either way.
    this.draft = provider ? { ...provider, keys: provider.keys.map((key) => ({ ...key })), models: [...provider.models] } : blank();
    this.keptKeys = Object.fromEntries((provider?.keys ?? []).map((key) => [key.id, key.apiKey]));
    this.keptKey = '';
    this.account = provider ? accountOf(FREE_CLOUD.find((preset) => samePreset(preset.base, provider.baseUrl))?.base ?? '', provider.baseUrl) : '';
    this.consoleSession = connectors.claudeCredit.sessionKey;
    this.providerModal = true;
    this.probeNote = null;
  }

  /** Opens the provider form for a Claude Console key: paste the key, Save; it becomes the Claude credit key. */
  useClaude() {
    this.draft = { ...blank(), name: 'Claude Console', kind: 'anthropic', baseUrl: ENDPOINTS.anthropic.api };
    this.keptKey = '';
    this.keptKeys = {};
    this.account = '';
    this.consoleSession = connectors.claudeCredit.sessionKey;
    this.providerModal = true;
    this.probeNote = { ok: true, text: 'Paste your Claude API key, then Save.' };
  }

  async test() {
    const draft = this.draft;
    if (!draft) return;
    this.probing = true;
    this.probeNote = null;
    try {
      // Workers AI's OpenAI-compatible address answers POST only: GET `/models` gets 405. Its model is the preset's.
      const preset = this.freePreset;
      if (preset?.base.includes('{account_id}') && preset.model) {
        draft.models = [preset.model];
        draft.defaultModel = preset.model;
        this.probeNote = { ok: true, text: '1 model (Workers AI preset)' };
        return;
      }
      const active = draft.keys.find((key) => key.id === draft.activeKeyId) ?? draft.keys[0];
      const result = await probeProvider(draft.kind, draft.baseUrl, active?.apiKey ?? '', active?.keyEnv ?? draft.keyEnv);
      if (result.ok) {
        draft.models = result.models;
        if (!result.models.includes(draft.defaultModel)) draft.defaultModel = result.models[0] ?? '';
        this.probeNote = { ok: true, text: `${result.models.length} model${result.models.length === 1 ? '' : 's'} in ${result.ms} ms` };
      } else {
        this.probeNote = { ok: false, text: result.error ?? 'failed' };
      }
    } finally {
      this.probing = false;
    }
  }

  async commit() {
    if (!this.draft) return;
    if (this.draft.baseUrl.includes('{account_id}')) {
      this.probeNote = { ok: false, text: 'Type the account ID first' };
      return;
    }
    // No model list yet: ask the server for it, so the provider shows up in the chat's model picker with its models.
    if (this.draft.models.length === 0 && this.draft.baseUrl.trim()) await this.test();
    const draft = this.draft;
    if (!draft) return;
    draft.name = this.fixedName ?? (draft.name.trim() || KINDS.find((kind) => kind.value === draft.kind)?.label || 'Provider');
    const saved = $state.snapshot(draft) as CustomProvider;
    saveProvider(saved);
    // The active key of a Claude Console provider is the credit key (Claude picker's "Claude credit" models).
    if (isClaudeConsole(saved)) {
      const active = saved.keys.find((key) => key.id === saved.activeKeyId) ?? saved.keys[0];
      connectors.claudeCredit = { ...connectors.claudeCredit, apiKey: active?.apiKey ?? '', sessionKey: this.consoleSession.trim().replace(/^sessionKey=/, ''), enabled: true };
      void refreshClaudeCredit();
    }
    this.providerModal = false;
  }

  addKey() {
    if (!this.draft) return;
    const id = `key-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
    this.draft.keys.push({ id, label: `Key ${this.draft.keys.length + 1}`, apiKey: '' });
    this.draft.activeKeyId ??= id;
  }

  removeKey(id: string) {
    if (!this.draft) return;
    this.draft.keys = this.draft.keys.filter((key) => key.id !== id);
    delete this.keptKeys[id];
    if (this.draft.activeKeyId === id) this.draft.activeKeyId = this.draft.keys[0]?.id ?? null;
  }

  /** Opens the provider form filled in for a free cloud API: paste the key, Test lists its models, Save. */
  useFreeCloud(preset: (typeof FREE_CLOUD)[number]) {
    this.draft = { ...blank(), name: preset.name, kind: 'openai', baseUrl: preset.base, keyEnv: preset.keyEnv, defaultModel: preset.model, restricted: true };
    this.keptKey = '';
    this.account = '';
    this.providerModal = true;
    this.probeNote = { ok: true, text: `Paste your ${preset.name} key (or set ${preset.keyEnv}), then Load models.` };
  }

  useLocal(server: { kind: CustomProvider['kind']; label: string; base_url: string; models: string[] }) {
    this.draft = { ...blank(), name: server.label, kind: server.kind, baseUrl: server.base_url, models: server.models, defaultModel: server.models[0] ?? '' };
    this.keptKey = '';
    this.providerModal = true;
    this.probeNote = { ok: true, text: `${server.models.length} models found` };
  }

  /** Reads an image file and keeps it as a small PNG `data:` URL, so a big upload never bloats the saved settings. */
  async pickLogo(file: File | undefined) {
    if (!file || !this.draft || !file.type.startsWith('image/')) return;
    const bitmap = await createImageBitmap(file).catch(() => null);
    if (!bitmap) return;
    const side = 64;
    const scale = Math.min(side / bitmap.width, side / bitmap.height, 1);
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    canvas.getContext('2d')?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    if (this.draft) this.draft.logo = canvas.toDataURL('image/png');
  }

  pickKind(value: string) {
    const draft = this.draft;
    if (!draft) return;
    const previous = KINDS.find((kind) => kind.value === draft.kind);
    draft.kind = value as CustomProvider['kind'];
    const next = KINDS.find((kind) => kind.value === value);
    if (next && (!draft.baseUrl || draft.baseUrl === previous?.base)) draft.baseUrl = next.base;
  }
}
