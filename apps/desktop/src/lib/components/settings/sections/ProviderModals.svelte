<script lang="ts">
  import { untrack } from 'svelte';
  import { ZButton, ZIcon, ZInput, ZModal, ZSelect, ZSwitch } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import SecretInput from '../../ui/SecretInput.svelte';
  import { ENDPOINTS } from '../../../agents/endpoints';
  import { brandLogoFor } from '../../../agents/brand-icons';
  import { connectors } from '../../../agents/connectors.svelte';
  import { KINDS, type ProviderForm } from './provider-form.svelte';

  let { form }: { form: ProviderForm } = $props();

  const icon = (name: string) => name as ZIconName;

  $effect(() => {
    const id = form.account.trim();
    untrack(() => {
      if (form.draft && form.freePreset?.base.includes('{account_id}')) form.draft.baseUrl = form.freePreset.base.replace('{account_id}', id || '{account_id}');
    });
  });
</script>

<!-- The provider form, over the settings (as adding an MCP server is). -->
<ZModal bind:open={form.providerModal} zTitle={form.draft && connectors.custom.some((item) => item.id === form.draft?.id) ? `Edit ${form.draft.name || 'provider'}` : form.freePreset ? `Add ${form.freePreset.name}` : 'New provider'} zWidth="640px" zBodyClass="scrollbar-full py-4" zOkText="Save provider" zCancelText="Cancel" zOkDisabled={!form.draft?.baseUrl.trim() || form.probing} zLoading={form.probing} onOk={() => void form.commit()} onClosed={() => (form.draft = null)}>
  {#if form.draft}
    {@const draft = form.draft}
    <form class="flex flex-col gap-5" onsubmit={(event) => { event.preventDefault(); void form.commit(); }}>
      <!-- One field per line: the label on the left, the control on the right. -->
      <section class="grid grid-cols-[7.5rem_minmax(0,1fr)] items-start gap-x-4 gap-y-4 rounded-xl border border-border bg-muted/20 p-4 text-sm">
        <span class="pt-2 font-medium">Name and logo</span>
        <div class="flex min-w-0 items-center gap-2.5">
          <!-- A known provider shows its own logo (no upload). A free cloud provider's name is read-only; a
               custom endpoint takes an uploaded logo and any name. -->
          {#if brandLogoFor(draft.baseUrl)}
            <span class="grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-muted/40"><img src={brandLogoFor(draft.baseUrl)} alt="" class="size-6 object-contain" /></span>
          {:else}
            <label class="group relative grid size-9 shrink-0 cursor-pointer place-items-center overflow-hidden rounded-lg border border-dashed border-border bg-muted/40 transition-colors hover:border-primary/60 hover:bg-primary/5" title="Upload a logo">
              {#if draft.logo}<img src={draft.logo} alt="" class="size-6 object-contain" />{:else}<ZIcon zType={icon('lucideUpload')} class="size-4 text-muted-foreground group-hover:text-primary" />{/if}
              <input type="file" accept="image/*" class="sr-only" onchange={(event) => { void form.pickLogo(event.currentTarget.files?.[0]); event.currentTarget.value = ''; }} />
            </label>
          {/if}
          {#if form.freePreset}
            <div class="min-w-0 flex-1" title="The name of a free cloud provider is fixed"><ZInput value={form.freePreset.name} zReadonly /></div>
          {:else}
            <div class="min-w-0 flex-1"><ZInput bind:value={draft.name} zPlaceholder="My LiteLLM" /></div>
          {/if}
          {#if draft.logo && !brandLogoFor(draft.baseUrl)}<button type="button" class="shrink-0 text-[0.75rem] text-muted-foreground hover:text-destructive" onclick={() => { if (form.draft) form.draft.logo = undefined; }}>Remove</button>{/if}
        </div>

        <span class="pt-2 font-medium">Protocol</span>
        <div class="flex min-w-0 flex-col gap-1">
          <ZSelect zOptions={KINDS.map((kind) => ({ label: kind.label, value: kind.value }))} value={draft.kind} onValueChange={(value) => form.pickKind(String(value))} />
          <span class="text-[0.75rem] text-muted-foreground">{KINDS.find((kind) => kind.value === draft?.kind)?.hint}</span>
        </div>

        {#if form.freePreset?.base.includes('{account_id}')}
          <span class="pt-2 font-medium">Account ID</span>
          <div class="flex min-w-0 flex-col gap-1">
            <ZInput bind:value={form.account} zPlaceholder="32 hex characters" />
            <span class="text-[0.75rem] text-muted-foreground">On the Cloudflare dashboard, under Workers &amp; Pages (right column). The key is an API token with Workers AI access.</span>
          </div>
        {/if}

        <span class="pt-2 font-medium">Base URL</span>
        <div class="min-w-0"><ZInput bind:value={draft.baseUrl} zPlaceholder={ENDPOINTS.providerKinds.openai} /></div>
      </section>

      <!-- Keys: each one is its own card; the active key is used first, and auto switch moves to the next when it fails. -->
      <section class="flex flex-col gap-3 rounded-xl border border-border bg-muted/20 p-4">
        <div class="flex items-center gap-3">
          <div class="min-w-0 flex-1">
            <div class="text-sm font-medium">API keys <span class="font-normal text-muted-foreground">· optional</span></div>
            <p class="mt-0.5 text-[0.75rem] text-muted-foreground">The key stays on this computer. It is never part of the source or a build.</p>
          </div>
          <ZButton type="button" zWave={false} zType="outline" zSize="sm" class="shrink-0 gap-1.5 px-3 text-sm" onclick={() => form.addKey()}><ZIcon zType={icon('lucidePlus')} class="size-3.5" /> Add key</ZButton>
        </div>

        {#each draft.keys as key (key.id)}
          {@const active = draft.activeKeyId === key.id}
          <div class="flex flex-col gap-3 rounded-lg border bg-background/60 p-3.5 transition-colors {active ? 'border-primary/50' : 'border-border'}">
            <div class="flex items-center gap-2.5">
              <input type="radio" name="active-provider-key" class="accent-primary" checked={active} aria-label="Use {key.label}" onchange={() => (draft.activeKeyId = key.id)} />
              <div class="min-w-0 flex-1"><ZInput bind:value={key.label} zPlaceholder="Key label" /></div>
              {#if active}<span class="shrink-0 rounded-full bg-primary/15 px-2 py-0.5 text-[0.6875rem] font-medium text-primary">Active</span>{/if}
              <ZButton type="button" zWave={false} zType="ghost" zSize="sm" class="size-8! shrink-0 p-0 text-muted-foreground hover:!text-destructive" aria-label="Remove {key.label}" onclick={() => form.removeKey(key.id)}><ZIcon zType={icon('lucideTrash2')} class="size-4" /></ZButton>
            </div>
            {#key key.id}<SecretInput bind:value={key.apiKey} kept={form.keptKeys[key.id] ?? ''} />{/key}
          </div>
        {:else}
          <p class="rounded-lg border border-dashed border-border px-3 py-4 text-center text-[0.75rem] text-muted-foreground">No key yet. Add one if the provider needs it.</p>
        {/each}

        <div class="flex items-center gap-3 rounded-lg border border-border bg-background/60 px-3 py-2">
          <div class="min-w-0 flex-1">
            <div class="text-sm font-medium">Auto switch keys</div>
            <p class="mt-0.5 text-[0.75rem] text-muted-foreground">Use the next key when quota or authentication fails.</p>
          </div>
          <ZSwitch zVariant="dot" checked={draft.autoSwitch} zSize="sm" onChange={(value) => (draft.autoSwitch = value)} />
        </div>
      </section>

      <!-- Models: load them from the endpoint, or type one id. -->
      <section class="rounded-xl border border-border bg-muted/20 p-4">
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-sm font-medium">Models</span>
          <ZButton zWave={false} zType="outline" zSize="sm" class="ml-auto h-8 gap-1.5 px-3 text-sm" disabled={form.probing || !draft.baseUrl} onclick={() => void form.test()}>
            <ZIcon zType={icon(form.probing ? 'lucideLoaderCircle' : 'lucideRefreshCw')} class="size-3.5 {form.probing ? 'animate-spin' : ''}" /> {draft.models.length ? 'Reload models' : 'Load models'}
          </ZButton>
        </div>
        <!-- One line always kept for the note, so loading, a result or an error never moves the form. -->
        <p class="mt-2 flex h-4 min-w-0 items-center gap-1 text-sm transition-colors duration-200 {form.probing ? 'text-muted-foreground' : form.probeNote?.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-destructive'}" role="status" title={form.probeNote?.text}>
          {#if form.probing}
            <ZIcon zType={icon('lucideLoaderCircle')} class="size-3.5 shrink-0 animate-spin" /><span class="truncate">Loading models…</span>
          {:else if form.probeNote}
            <ZIcon zType={icon(form.probeNote.ok ? 'lucideCircleCheck' : 'lucideCircleX')} class="size-3.5 shrink-0" /><span class="truncate">{form.probeNote.text}</span>
          {/if}
        </p>
        {#if draft.models.length > 0}
          <div class="mt-3 grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-x-4 text-sm">
            <span class="text-muted-foreground">Default model</span>
            <ZSelect zOptions={draft.models.map((model) => ({ label: model, value: model }))} zShowSearch={draft.models.length > 8} value={draft.defaultModel} onValueChange={(value) => { if (draft) draft.defaultModel = String(value ?? ''); }} />
          </div>
        {:else}
          <div class="mt-3 grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-x-4 text-sm">
            <span class="text-muted-foreground">Model id</span>
            <ZInput bind:value={draft.defaultModel} zPlaceholder="qwen3-coder:30b" />
          </div>
          <p class="mt-1.5 text-[0.75rem] text-muted-foreground">Loaded automatically on save, or type one.</p>
        {/if}
      </section>

      <!-- Privacy: a free plan may keep what it is sent, so the agents are limited. -->
      <section class="flex items-start gap-4 rounded-xl border border-border p-4">
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium">Free tier: keep private data away</div>
          <p class="mt-1 text-[0.75rem] leading-relaxed text-muted-foreground">Free plans may keep and train on what they are sent. This provider then gets web search only: no files, code, terminal, browser or MCP tools, and no project text. Other agents may hand it light public work (search, emails, prompts, scripts) after checking it holds nothing private.</p>
        </div>
        <ZSwitch zVariant="dot" checked={Boolean(draft.restricted)} zSize="sm" onChange={(value) => { if (draft) draft.restricted = value; }} />
      </section>
    </form>
  {/if}
</ZModal>
