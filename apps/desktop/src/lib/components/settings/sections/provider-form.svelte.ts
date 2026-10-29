import { ENDPOINTS } from '../../../agents/endpoints';
import { newProviderId, probeProvider, saveProvider, type CustomProvider } from '../../../agents/connectors.svelte';

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

const blank = (): CustomProvider => ({ id: newProviderId(), name: '', kind: 'openai', baseUrl: KINDS[0].base, keyEnv: '', apiKey: '', models: [], defaultModel: '', enabled: true });

/**
 * State and actions of the provider form, shared by the providers list (which opens it) and the form modals
 * (which stay in the settings shell so they sit above the panel).
 */
export class ProviderForm {
  draft = $state<CustomProvider | null>(null);
  probing = $state(false);
  probeNote = $state<{ ok: boolean; text: string } | null>(null);
  confirmDelete = $state('');
  /** The key the provider being edited was saved with: shown hidden, never in the field (SecretInput). */
  keptKey = $state('');
  /** The provider form is a modal over the settings (like adding an MCP server). */
  providerModal = $state(false);
  account = $state('');
  /** The free cloud provider being edited, found by its address: its name and logo are fixed. */
  // A getter, not $derived: this class is made inside the settings modal, and a $derived would belong to that component's
  // effect scope, so reading it after the modal closes warns (derived_inert). A getter still reads the same state.
  get freePreset() {
    return this.draft ? FREE_CLOUD.find((preset) => samePreset(preset.base, this.draft?.baseUrl ?? '') || preset.base === this.draft?.baseUrl) : undefined;
  }

  edit(provider?: CustomProvider) {
    // A provider saved before keys were kept as text has none: the field binds a string either way.
    this.draft = provider ? { ...provider, apiKey: provider.apiKey ?? '', models: [...provider.models] } : blank();
    this.keptKey = provider?.apiKey ?? '';
    this.account = provider ? accountOf(FREE_CLOUD.find((preset) => samePreset(preset.base, provider.baseUrl))?.base ?? '', provider.baseUrl) : '';
    this.providerModal = true;
    this.probeNote = null;
  }

  async test() {
    const draft = this.draft;
    if (!draft) return;
    this.probing = true;
    this.probeNote = null;
    try {
      const result = await probeProvider(draft.kind, draft.baseUrl, draft.apiKey ?? '', draft.keyEnv);
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
    draft.name = this.freePreset?.name ?? (draft.name.trim() || KINDS.find((kind) => kind.value === draft.kind)?.label || 'Provider');
    saveProvider($state.snapshot(draft) as CustomProvider);
    this.providerModal = false;
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
